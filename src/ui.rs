use super::trace;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContext, EguiPlugin};
use egui::Slider;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(EguiPlugin).add_system(ui_system);
    }
}

fn ui_system(mut egui_context: ResMut<EguiContext>, mut uniforms: ResMut<trace::Uniforms>) {
    egui::Window::new("Settings")
        .anchor(egui::Align2::RIGHT_TOP, [-5.0, 5.0])
        .show(egui_context.ctx_mut(), |ui| {
            ui.checkbox(&mut uniforms.show_ray_steps, "Show ray steps");
            ui.add(
                Slider::new(&mut uniforms.accumulation_frames, 1.0..=100.0)
                    .text("Accumulation frames"),
            );
            ui.checkbox(&mut uniforms.freeze, "Freeze");
            ui.checkbox(&mut uniforms.misc_bool, "Misc bool");
            ui.add(Slider::new(&mut uniforms.misc_float, 0.0..=1.0).text("Misc float"));
        });
}