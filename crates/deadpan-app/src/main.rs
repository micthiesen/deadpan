//! Native source preview and the shared headless command entrypoint.

mod dialogs;
mod library;
mod navigation;
mod presentation;
mod preview;
mod project;
mod transport;
mod worker;

use std::cell::Cell;
use std::rc::Rc;

use eframe::egui;
use preview::DeadpanApp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|argument| argument == "--headless")
    {
        let result = deadpan_cli::entry(arguments.into_iter().skip(1));
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
            return Ok(());
        }
        _ => {
            return Err(
                "Usage: deadpan-app [--smoke-test | --project PATH | --preview-source PATH | --headless <command>]"
                    .into(),
            );
        }
    };

    let exited = Rc::new(Cell::new(false));
    let exit_observer = Rc::clone(&exited);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 640.0])
            .with_icon(egui::IconData::default()),
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
            Ok(Box::new(DeadpanApp::new(
                context,
                render_state.clone(),
                smoke_test,
                exit_observer,
                preview_source,
                project,
            )?))
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
