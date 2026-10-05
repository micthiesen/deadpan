//! Native source preview and the shared headless command entrypoint.

mod dialogs;
mod gain;
mod keymap;
mod keymap_file;
mod library;
#[cfg(target_os = "macos")]
mod menu;
mod model_packs;
mod navigation;
mod presentation;
mod preview;
mod project;
mod recovery;
mod transport;
#[cfg(feature = "ui-harness")]
mod ui_harness;
mod worker;
mod youtube;

use std::cell::Cell;
use std::rc::Rc;

use eframe::egui;
use preview::DeadpanApp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|argument| argument == "--ui-check")
    {
        #[cfg(feature = "ui-harness")]
        return ui_harness::entry(&arguments[1..]).map_err(Into::into);
        #[cfg(not(feature = "ui-harness"))]
        return Err("UI replay requires the developer build: cargo run -p deadpan-app --features ui-harness -- --ui-check --help".into());
    }
    let headless = arguments
        .first()
        .is_some_and(|argument| argument == "--headless");
    let private_worker = match arguments.as_slice() {
        [operation, _] => {
            operation == deadpan_cli::render_worker::PRIVATE_WORKER_ARGUMENT
                || operation == deadpan_cli::encoded_render::PRIVATE_WORKER_ARGUMENT
        }
        [operation] => {
            operation == deadpan_cli::encoded_render::verification::PRIVATE_WORKER_ARGUMENT
                || operation == deadpan_cli::encoded_render::admission::PRIVATE_WORKER_ARGUMENT
        }
        _ => false,
    };
    if headless || private_worker {
        let result = deadpan_cli::entry(arguments.into_iter().skip(usize::from(headless)));
        if result == std::process::ExitCode::SUCCESS {
            return Ok(());
        }
        std::process::exit(1);
    }
    let (smoke_test, preview_source, project) = match arguments.as_slice() {
        [] => (false, None, None),
        [argument] if argument == "--smoke-test" => (true, None, None),
        [argument, path] if argument == "--preview-source" => (false, Some(path.clone()), None),
        [argument, path] if argument == "--project" => (false, None, Some(path.clone())),
        [argument] if argument == "--help" || argument == "-h" => {
            println!(
                "Usage: deadpan-app [--smoke-test | --project PATH | --preview-source PATH | --headless <command>]\n\nOpen a project workspace, preview a source, or run headless project commands."
            );
            #[cfg(feature = "ui-harness")]
            println!("Developer UI replay: deadpan-app --ui-check --help");
            return Ok(());
        }
        _ => {
            return Err(
                "Usage: deadpan-app [--smoke-test | --project PATH | --preview-source PATH | --headless <command>]"
                    .into(),
            );
        }
    };

    // Admit the entire immutable map before the first native input batch. Replay
    // and lifecycle qualification use explicit shipped maps, never personal files.
    let keymap = if smoke_test {
        keymap::Startup::shipped()
    } else {
        keymap::Startup::from_file(keymap_file::load())
    };
    let opens_project = project.is_some() || preview_source.is_some();
    let exited = Rc::new(Cell::new(false));
    let exit_observer = Rc::clone(&exited);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 640.0])
            .with_icon(application_icon()?),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Deadpan",
        options,
        Box::new(move |context| {
            let render_state = context
                .wgpu_render_state
                .as_ref()
                .ok_or("The native GPU renderer did not initialize")?;
            let adapter = render_state.adapter.get_info();
            #[cfg(target_os = "macos")]
            if adapter.backend != eframe::wgpu::Backend::Metal {
                return Err("Deadpan requires the Metal GPU backend on macOS".into());
            }
            if smoke_test {
                println!(
                    "Native GPU initialized: {} ({:?})",
                    adapter.name, adapter.backend
                );
            }
            #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
            let mut app = DeadpanApp::new(
                context,
                render_state.clone(),
                smoke_test,
                exit_observer,
                preview_source,
                project,
                keymap,
            )?;
            if !smoke_test {
                // Offer the project an earlier launch left open, unless one
                // was named explicitly on the command line.
                match recovery::LaunchJournal::user() {
                    Ok(journal) => app.use_launch_journal(journal, !opens_project),
                    Err(error) => eprintln!("Crash recovery is unavailable: {error}"),
                }
            }
            #[cfg(target_os = "macos")]
            app.install_menu(&context.egui_ctx)?;
            Ok(Box::new(app))
        }),
    )?;

    if smoke_test {
        if !exited.get() {
            return Err("The native lifecycle did not complete its shutdown callback".into());
        }
        println!("Native window smoke test passed; shutdown callback completed.");
    }
    Ok(())
}

fn application_icon() -> Result<egui::IconData, Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSBundle, NSString};

        let bundle = NSBundle::mainBundle();
        if ["CFBundleIconName", "CFBundleIconFile"].iter().any(|key| {
            bundle
                .objectForInfoDictionaryKey(&NSString::from_str(key))
                .is_some()
        }) {
            // Let macOS render the bundle's appearance-aware icon. An explicit
            // bitmap would replace it through NSApplication on every launch.
            return Ok(egui::IconData::default());
        }
    }
    // Bare Cargo launches have no bundle resources, but still need a Dock icon.
    Ok(eframe::icon_data::from_png_bytes(include_bytes!(
        "../../../assets/brand/app-icon-256.png"
    ))?)
}
