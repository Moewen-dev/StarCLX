//! Callback-Modul der Anlage: eine Webseite unter `<Anlage>/callback`.
//!
//! Die Seite verbietet das Einbetten per iframe (`X-Frame-Options: DENY`,
//! `frame-ancestors 'self'`, Sitzungscookie `SameSite=Strict`). Der Reiter
//! „Callback“ zeigt sie deshalb in einem eigenen Webview, das als Kind im
//! Hauptfenster genau über der Fläche des Reiters liegt; für die Seite ist das
//! ein gewöhnlicher Browser-Tab. Alternativ lässt sie sich in einem eigenen
//! Fenster öffnen. Beide nehmen wie das Anmeldefenster Zertifikate an, die der
//! Benutzer bestätigt hat.

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl, WebviewWindowBuilder};

/// Eigenes Fenster
const WINDOW: &str = "callback";
/// Eingebettetes Webview im Hauptfenster
const EMBED: &str = "callback-embed";

/// Adresse des Callback-Moduls einer Anlage
pub fn url_of(server: &str) -> Result<url::Url, String> {
    let base = server.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Keine Anlage angemeldet".into());
    }
    let base = if base.contains("://") { base.to_owned() } else { format!("https://{base}") };
    let url = url::Url::parse(&format!("{base}/callback/")).map_err(|e| e.to_string())?;
    match url.scheme() {
        "https" | "http" => Ok(url),
        other => Err(format!("Nicht unterstütztes Schema: {other}")),
    }
}

/// Zeigt das eingebettete Webview an der Stelle des Reiters (logische
/// Pixel relativ zum Inhalt des Hauptfensters). Legt es beim ersten Mal an
/// und lädt die Seite neu, wenn sich die Anlage geändert hat.
#[tauri::command]
pub async fn callback_show(
    app: AppHandle,
    server: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let url = url_of(&server)?;
    let position = LogicalPosition::new(x.max(0.0), y.max(0.0));
    let size = LogicalSize::new(width.max(1.0), height.max(1.0));
    let view = match app.get_webview(EMBED) {
        Some(view) => {
            // Andere Anlage (Kontowechsel): neu laden
            let current = view.url().map_err(|e| e.to_string())?;
            if current.host_str() != url.host_str() || current.port() != url.port() {
                view.navigate(url).map_err(|e| e.to_string())?;
            }
            view
        }
        None => {
            let window = app.get_window("main").ok_or("Hauptfenster fehlt")?;
            let view = window
                .add_child(
                    WebviewBuilder::new(EMBED, WebviewUrl::External("about:blank".parse().unwrap())),
                    position,
                    size,
                )
                .map_err(|e| e.to_string())?;
            #[cfg(target_os = "linux")]
            view.with_webview(|w| {
                use gtk::prelude::WidgetExt;
                w.inner().set_widget_name(GTK_NAME);
                crate::login::allow_confirmed_certificates(w);
            })
            .map_err(|e| e.to_string())?;
            view.navigate(url).map_err(|e| e.to_string())?;
            view
        }
    };
    #[cfg(target_os = "linux")]
    {
        // GTK ignoriert die Position (Kind landet in einer GtkBox), deshalb
        // selbst in ein Overlay über dem Hauptwebview legen.
        let (x, y) = (position.x.round() as i32, position.y.round() as i32);
        let (w, h) = (size.width.round() as i32, size.height.round() as i32);
        app.get_webview("main")
            .ok_or("Hauptwebview fehlt")?
            .with_webview(move |main| overlay::place(main.inner(), x, y, w, h))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "linux"))]
    {
        view.set_position(position).map_err(|e| e.to_string())?;
        view.set_size(size).map_err(|e| e.to_string())?;
    }
    view.show().map_err(|e| e.to_string())
}

/// Name des GTK-Widgets, an dem das eingebettete Webview erkannt wird
#[cfg(target_os = "linux")]
const GTK_NAME: &str = "starclx-callback-embed";

#[cfg(target_os = "linux")]
mod overlay {
    //! Tauri hängt Kind-Webviews unter Linux mit `pack_start` an die GtkBox
    //! des Fensters an, also *unter* das Hauptwebview, und ignoriert die
    //! Position. Hier wird das Hauptwebview einmalig in ein `gtk::Overlay`
    //! umgehängt und das Callback-Webview als Overlay darüber gelegt;
    //! Lage und Größe folgen über Ränder und `size_request`.
    use gtk::prelude::*;

    pub fn place(main: webkit2gtk::WebView, x: i32, y: i32, w: i32, h: i32) {
        let main: gtk::Widget = main.upcast();
        let Some(parent) = main.parent() else {
            tracing::warn!("Callback: Hauptwebview ohne Elternelement");
            return;
        };
        let child = match parent.downcast_ref::<gtk::Overlay>() {
            Some(overlay) => find(overlay.upcast_ref()),
            None => {
                let Some(vbox) = parent.downcast_ref::<gtk::Box>() else {
                    tracing::warn!(parent = %parent.type_().name(), "Callback: unerwartetes Elternelement");
                    return;
                };
                let Some(child) = find(vbox.upcast_ref()) else {
                    tracing::warn!("Callback: eingebettetes Webview nicht gefunden");
                    return;
                };
                let overlay = gtk::Overlay::new();
                vbox.remove(&child);
                vbox.remove(&main);
                overlay.add(&main);
                overlay.add_overlay(&child);
                vbox.pack_start(&overlay, true, true, 0);
                overlay.show();
                main.show();
                Some(child)
            }
        };
        let Some(child) = child else {
            tracing::warn!("Callback: eingebettetes Webview nicht gefunden");
            return;
        };
        child.set_halign(gtk::Align::Start);
        child.set_valign(gtk::Align::Start);
        child.set_margin_start(x);
        child.set_margin_top(y);
        child.set_size_request(w, h);
        child.show();
    }

    fn find(container: &gtk::Container) -> Option<gtk::Widget> {
        container
            .children()
            .into_iter()
            .find(|c| c.widget_name() == super::GTK_NAME)
    }
}

/// Blendet das eingebettete Webview aus (Reiter verlassen, Menü offen …).
#[tauri::command]
pub async fn callback_hide(app: AppHandle) -> Result<(), String> {
    if let Some(view) = app.get_webview(EMBED) {
        view.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Lädt das Callback-Modul im eingebetteten Webview neu.
#[tauri::command]
pub async fn callback_reload(app: AppHandle, server: String) -> Result<(), String> {
    let url = url_of(&server)?;
    if let Some(view) = app.get_webview(EMBED) {
        view.navigate(url).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Öffnet das Callback-Modul in einem eigenen Fenster oder holt es nach vorn.
#[tauri::command]
pub async fn callback_open(app: AppHandle, server: String) -> Result<(), String> {
    let url = url_of(&server)?;
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(
        &app,
        WINDOW,
        WebviewUrl::External("about:blank".parse().unwrap()),
    );
    let window = crate::instances::webview(&app, builder)
        .title(crate::i18n::t("StarCLX – Callback"))
        .inner_size(1000.0, 760.0)
        .build()
        .map_err(|e| e.to_string())?;
    crate::instances::set_icon(&app, &window);
    #[cfg(target_os = "linux")]
    window
        .with_webview(crate::login::allow_confirmed_certificates)
        .map_err(|e| e.to_string())?;
    window.navigate(url).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::url_of;

    #[test]
    fn haengt_callback_an() {
        assert_eq!(url_of("https://pbx.firma.de").unwrap().as_str(), "https://pbx.firma.de/callback/");
        assert_eq!(url_of("https://pbx.firma.de/").unwrap().as_str(), "https://pbx.firma.de/callback/");
        assert_eq!(url_of("pbx.firma.de:8443").unwrap().as_str(), "https://pbx.firma.de:8443/callback/");
    }

    #[test]
    fn lehnt_unsinn_ab() {
        assert!(url_of("").is_err());
        assert!(url_of("file:///etc").is_err());
    }
}
