//! Native host bootstrap. The editor and media pipeline are not implemented yet.

use std::cell::Cell;
use std::rc::Rc;

use eframe::egui;

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
    let smoke_test = match arguments.as_slice() {
        [] => false,
        [argument] if argument == "--smoke-test" => true,
        [argument] if argument == "--help" || argument == "-h" => {
            println!(
                "Usage: deadpan-app [--smoke-test | --headless <command>]\n\nOpen the native development shell or run headless project commands."
            );
            return Ok(());
        }
        _ => return Err("Usage: deadpan-app [--smoke-test | --headless <command>]".into()),
    };

    let exited = Rc::new(Cell::new(false));
    let exit_observer = Rc::clone(&exited);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([640.0, 420.0])
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
            Ok(Box::new(DeadpanApp {
                smoke_frames: smoke_test.then_some(0),
                exited: exit_observer,
            }))
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

struct DeadpanApp {
    smoke_frames: Option<u8>,
    exited: Rc<Cell<bool>>,
}

impl eframe::App for DeadpanApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(72.0);
                ui.vertical_centered(|ui| {
                    ui.heading(egui::RichText::new("Deadpan").size(48.0));
                    ui.add_space(12.0);
                    ui.label(
                        "A keyboard-native editor for making a moment last considerably too long.",
                    );
                    ui.add_space(48.0);
                    ui.strong("Development foundation");
                    ui.add_space(8.0);
                    ui.label("The editing workspace is not available in this build.");
                    ui.label("Import, playback, editing, AI holds, and export are still to come.");
                    ui.add_space(32.0);
                    if ui.button("Close").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });

        if let Some(frames) = self.smoke_frames.as_mut() {
            *frames += 1;
            if *frames >= 3 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ui.ctx().request_repaint();
            }
        }
    }

    fn on_exit(&mut self) {
        self.exited.set(true);
    }
}
