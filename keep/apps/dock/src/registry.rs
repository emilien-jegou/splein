// Single responsibility: App registry — what lives in the dock.
//
// Port of `backp/src/constants/apps.js` (`apps` + `dockLayout`).
// Labels stay English identifiers; i18n (`t(app.<id>)`) is a follow-up.

/// One dockable app.
pub struct App {
    pub id: &'static str,
    pub name: &'static str,
}

/// One slot in the dock: an app or a separator (`"|"` in `dockLayout`).
pub enum Slot {
    App(&'static App),
    Separator,
}

pub const APPS: &[App] = &[
    App { id: "finder", name: "Finder" },
    App { id: "safari", name: "Projects" },
    App { id: "notes", name: "Notes" },
    App { id: "photos", name: "Photos" },
    App { id: "terminal", name: "Terminal" },
    App { id: "vscode", name: "VS Code" },
    App { id: "contact", name: "Contacts" },
    App { id: "resume", name: "Resume" },
    App { id: "settings", name: "Settings" },
    App { id: "mail", name: "Mail" },
    App { id: "github", name: "GitHub" },
    App { id: "telegram", name: "Telegram" },
    App { id: "trash", name: "Trash" },
];

fn by_id(id: &str) -> &'static App {
    APPS.iter().find(|a| a.id == id).expect("dock registry: unknown app id")
}

/// macOS dock order. `None` renders a separator (the `"|"` entries).
const ORDER: &[Option<&str>] = &[
    Some("finder"),
    Some("safari"),
    Some("notes"),
    Some("photos"),
    Some("terminal"),
    Some("vscode"),
    Some("contact"),
    Some("resume"),
    Some("settings"),
    None,
    Some("mail"),
    Some("github"),
    Some("telegram"),
    None,
    Some("trash"),
];

/// Dock slots in display order.
pub fn slots() -> Vec<Slot> {
    ORDER
        .iter()
        .map(|entry| match entry {
            Some(id) => Slot::App(by_id(id)),
            None => Slot::Separator,
        })
        .collect()
}
