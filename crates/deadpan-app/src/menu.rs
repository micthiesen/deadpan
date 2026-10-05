//! Native macOS menu bar.
//!
//! Items dispatch the same editor actions as their keys and buttons. The bar
//! makes those actions searchable through the Help menu and reachable with
//! VoiceOver menu navigation. Menu key equivalents act before the window sees
//! a key, so only commands that never belong to a focused text field carry
//! them, and the app disables those items while text entry is active. Undo and
//! Redo keep their window routing so native field editing retains ⌘Z.

use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};

/// A command chosen from the menu bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCommand {
    New,
    NewFromUrl,
    Open,
    Import,
    Close,
    Render,
    Renders,
    Undo,
    Redo,
    ViewOriginal,
    ViewEdit,
    Keys,
    Models,
    Quit,
}

const COMMANDS: [(&str, MenuCommand); 14] = [
    ("deadpan.file.new", MenuCommand::New),
    ("deadpan.file.new-url", MenuCommand::NewFromUrl),
    ("deadpan.file.open", MenuCommand::Open),
    ("deadpan.file.import", MenuCommand::Import),
    ("deadpan.file.close", MenuCommand::Close),
    ("deadpan.file.render", MenuCommand::Render),
    ("deadpan.file.renders", MenuCommand::Renders),
    ("deadpan.edit.undo", MenuCommand::Undo),
    ("deadpan.edit.redo", MenuCommand::Redo),
    ("deadpan.view.original", MenuCommand::ViewOriginal),
    ("deadpan.view.edit", MenuCommand::ViewEdit),
    ("deadpan.help.keys", MenuCommand::Keys),
    ("deadpan.app.models", MenuCommand::Models),
    ("deadpan.app.quit", MenuCommand::Quit),
];

fn command(id: &MenuId) -> Option<MenuCommand> {
    COMMANDS
        .iter()
        .find(|(name, _)| id.as_ref() == *name)
        .map(|(_, command)| *command)
}

/// What the menu should currently allow, captured once per frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuState {
    /// No modal work, dialog, draft or text entry owns the keyboard.
    pub ready: bool,
    pub project: bool,
    pub importing: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The project has its Original, so ⌘I adds sounds.
    pub single_original_ready: bool,
    /// The project is waiting for its Original.
    pub awaiting_original: bool,
    /// The keyboard reference may open (not during a Gain draft or a
    /// blocking render).
    pub help_allowed: bool,
}

impl MenuState {
    fn import_label(&self) -> &'static str {
        if self.awaiting_original {
            "Choose Original…"
        } else if self.single_original_ready {
            "Add Sound…"
        } else {
            "Import Media…"
        }
    }

    pub fn enabled(&self, command: MenuCommand) -> bool {
        match command {
            MenuCommand::New | MenuCommand::NewFromUrl | MenuCommand::Open => self.ready,
            MenuCommand::Import => self.ready && self.project && !self.importing,
            MenuCommand::Close | MenuCommand::Render | MenuCommand::Renders => {
                self.ready && self.project
            }
            MenuCommand::Undo => self.ready && self.can_undo,
            MenuCommand::Redo => self.ready && self.can_redo,
            MenuCommand::ViewOriginal | MenuCommand::Models => self.ready,
            MenuCommand::ViewEdit => self.ready && self.project,
            MenuCommand::Keys => self.help_allowed,
            MenuCommand::Quit => true,
        }
    }
}

pub struct MenuBar {
    _menu: Menu,
    // A muda event handler replaces its global channel, so the handler
    // forwards chosen items here and wakes the window.
    events: std::sync::mpsc::Receiver<MenuId>,
    items: Vec<(MenuCommand, MenuItem)>,
    import: MenuItem,
    state: Option<MenuState>,
}

fn item(name: &str, title: &str, accelerator: Option<Accelerator>) -> MenuItem {
    MenuItem::with_id(name, title, true, accelerator)
}

fn command_key(code: Code) -> Option<Accelerator> {
    Some(Accelerator::new(Modifiers::META, code))
}

impl MenuBar {
    /// Replace the application's main menu. Call once on the main thread after
    /// the native event loop has created the application.
    pub fn install(repaint: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        let error = |error: muda::Error| error.to_string();
        let new = item("deadpan.file.new", "New Project…", command_key(Code::KeyN));
        let new_url = item(
            "deadpan.file.new-url",
            "New from YouTube URL…",
            Some(Accelerator::new(
                Modifiers::META | Modifiers::SHIFT,
                Code::KeyN,
            )),
        );
        let open = item(
            "deadpan.file.open",
            "Open Project…",
            command_key(Code::KeyO),
        );
        let import = item(
            "deadpan.file.import",
            "Import Media…",
            command_key(Code::KeyI),
        );
        let close = item("deadpan.file.close", "Close Project", None);
        let render = item("deadpan.file.render", "Render…", command_key(Code::KeyE));
        let renders = item("deadpan.file.renders", "Saved Renders…", None);
        let undo = item("deadpan.edit.undo", "Undo", None);
        let redo = item("deadpan.edit.redo", "Redo", None);
        let original = item("deadpan.view.original", "Original", None);
        let edit = item("deadpan.view.edit", "Your Edit", None);
        let keys = item("deadpan.help.keys", "Keyboard Reference", None);
        // Packs are global, so the panel needs no project.
        let models = item("deadpan.app.models", "Models…", None);
        // Quit asks the window to close so unfinished project work completes
        // through the app's ordinary close path.
        let quit = item("deadpan.app.quit", "Quit Deadpan", command_key(Code::KeyQ));

        let app = Submenu::with_items(
            "Deadpan",
            true,
            &[
                &PredefinedMenuItem::about(Some("About Deadpan"), None),
                &PredefinedMenuItem::separator(),
                &models,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(Some("Hide Deadpan")),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &quit,
            ],
        )
        .map_err(error)?;
        let file = Submenu::with_items(
            "File",
            true,
            &[
                &new,
                &new_url,
                &open,
                &PredefinedMenuItem::separator(),
                &import,
                &PredefinedMenuItem::separator(),
                &render,
                &renders,
                &PredefinedMenuItem::separator(),
                &close,
            ],
        )
        .map_err(error)?;
        let edit_menu = Submenu::with_items("Edit", true, &[&undo, &redo]).map_err(error)?;
        let view = Submenu::with_items("View", true, &[&original, &edit]).map_err(error)?;
        let window = Submenu::with_items(
            "Window",
            true,
            &[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::maximize(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::fullscreen(None),
            ],
        )
        .map_err(error)?;
        let help = Submenu::with_items("Help", true, &[&keys]).map_err(error)?;
        let menu =
            Menu::with_items(&[&app, &file, &edit_menu, &view, &window, &help]).map_err(error)?;
        menu.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
        help.set_as_help_menu_for_nsapp();
        let (sender, events) = std::sync::mpsc::channel();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            // The window owns the receiver for the application's lifetime;
            // after shutdown there is nothing left to notify.
            if sender.send(event.id().clone()).is_ok() {
                repaint();
            }
        }));
        Ok(Self {
            _menu: menu,
            events,
            items: vec![
                (MenuCommand::New, new),
                (MenuCommand::NewFromUrl, new_url),
                (MenuCommand::Open, open),
                (MenuCommand::Import, import.clone()),
                (MenuCommand::Close, close),
                (MenuCommand::Render, render),
                (MenuCommand::Renders, renders),
                (MenuCommand::Undo, undo),
                (MenuCommand::Redo, redo),
                (MenuCommand::ViewOriginal, original),
                (MenuCommand::ViewEdit, edit),
                (MenuCommand::Keys, keys),
                (MenuCommand::Models, models),
                (MenuCommand::Quit, quit),
            ],
            import,
            state: None,
        })
    }

    /// Commands chosen since the previous frame, in order.
    pub fn take_commands(&self) -> Vec<MenuCommand> {
        std::iter::from_fn(|| self.events.try_recv().ok())
            .filter_map(|id| command(&id))
            .collect()
    }

    /// Apply enablement and the context-dependent import title when they change.
    pub fn update(&mut self, state: MenuState) {
        if self.state.as_ref() == Some(&state) {
            return;
        }
        for (command, item) in &self.items {
            item.set_enabled(state.enabled(*command));
        }
        self.import.set_text(state.import_label());
        self.state = Some(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> MenuState {
        MenuState {
            ready: true,
            project: true,
            importing: false,
            can_undo: true,
            can_redo: false,
            single_original_ready: true,
            awaiting_original: false,
            help_allowed: true,
        }
    }

    #[test]
    fn every_identifier_maps_to_one_distinct_command() {
        for (index, (name, expected)) in COMMANDS.iter().enumerate() {
            assert_eq!(command(&MenuId::new(name)), Some(*expected));
            assert!(
                COMMANDS[index + 1..]
                    .iter()
                    .all(|(other, command)| other != name && command != expected)
            );
        }
        assert_eq!(command(&MenuId::new("deadpan.unknown")), None);
    }

    #[test]
    fn busy_text_entry_and_missing_project_disable_their_commands() {
        let ready = state();
        assert!(ready.enabled(MenuCommand::Render));
        assert!(ready.enabled(MenuCommand::Undo));
        assert!(!ready.enabled(MenuCommand::Redo));
        let busy = MenuState {
            ready: false,
            ..state()
        };
        for (_, command) in COMMANDS {
            assert_eq!(
                busy.enabled(command),
                matches!(command, MenuCommand::Keys | MenuCommand::Quit)
            );
        }
        let gain = MenuState {
            ready: false,
            help_allowed: false,
            ..state()
        };
        assert!(!gain.enabled(MenuCommand::Keys));
        assert!(gain.enabled(MenuCommand::Quit));
        let empty = MenuState {
            project: false,
            ..state()
        };
        assert!(empty.enabled(MenuCommand::New));
        assert!(empty.enabled(MenuCommand::NewFromUrl));
        assert!(empty.enabled(MenuCommand::ViewOriginal));
        assert!(empty.enabled(MenuCommand::Models));
        assert!(!empty.enabled(MenuCommand::Import));
        assert!(!empty.enabled(MenuCommand::Render));
        assert!(!empty.enabled(MenuCommand::ViewEdit));
        let importing = MenuState {
            importing: true,
            ..state()
        };
        assert!(!importing.enabled(MenuCommand::Import));
    }

    #[test]
    fn import_title_follows_the_single_original_workflow() {
        assert_eq!(state().import_label(), "Add Sound…");
        let awaiting = MenuState {
            awaiting_original: true,
            single_original_ready: false,
            ..state()
        };
        assert_eq!(awaiting.import_label(), "Choose Original…");
        let legacy = MenuState {
            single_original_ready: false,
            ..state()
        };
        assert_eq!(legacy.import_label(), "Import Media…");
    }
}
