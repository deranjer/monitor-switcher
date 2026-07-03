use crate::controller::LogEntry;

pub fn show(ui: &mut egui::Ui, log: &[LogEntry]) {
    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
        for entry in log {
            ui.label(format!("[{}] {}", entry.timestamp, entry.message));
        }
        if log.is_empty() {
            ui.label("No activity yet.");
        }
    });
}
