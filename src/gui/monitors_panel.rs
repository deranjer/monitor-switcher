use std::rc::Rc;

use winsafe::gui;
use winsafe::prelude::*;

use super::{MainWindow, PANEL_H, PANEL_W};
use crate::platform::windows::monitor_identity::enumerate_monitor_geometry;

pub struct MonitorsPanel {
    pub panel: gui::WindowControl,
    pub list: gui::ListBox,
    pub detect_btn: gui::Button,
    pub identify_btn: gui::Button,
    pub detail: gui::Edit,
    pub verify_chk: gui::CheckBox,
}

pub fn build(parent: &(impl GuiParent + 'static)) -> MonitorsPanel {
    let panel = gui::WindowControl::new(
        parent,
        gui::WindowControlOpts {
            position: (0, 0),
            size: (PANEL_W, PANEL_H),
            ex_style: winsafe::co::WS_EX::LEFT | winsafe::co::WS_EX::CONTROLPARENT,
            ..Default::default()
        },
    );

    let detect_btn = gui::Button::new(
        &panel,
        gui::ButtonOpts { position: (10, 10), text: "Detect / Refresh".to_owned(), ..Default::default() },
    );
    let identify_btn = gui::Button::new(
        &panel,
        gui::ButtonOpts { position: (160, 10), text: "Identify Monitors".to_owned(), ..Default::default() },
    );

    let list = gui::ListBox::new(
        &panel,
        gui::ListBoxOpts { position: (10, 45), size: (150, PANEL_H - 60), ..Default::default() },
    );

    let detail = gui::Edit::new(
        &panel,
        gui::EditOpts {
            position: (170, 45),
            width: PANEL_W - 185,
            height: PANEL_H - 100,
            control_style: winsafe::co::ES::MULTILINE | winsafe::co::ES::AUTOVSCROLL | winsafe::co::ES::READONLY,
            window_style: winsafe::co::WS::CHILD | winsafe::co::WS::VISIBLE | winsafe::co::WS::VSCROLL | winsafe::co::WS::TABSTOP,
            ..Default::default()
        },
    );

    let verify_chk = gui::CheckBox::new(
        &panel,
        gui::CheckBoxOpts {
            position: (170, PANEL_H - 45),
            text: "Verify switches on this monitor".to_owned(),
            ..Default::default()
        },
    );

    MonitorsPanel { panel, list, detect_btn, identify_btn, detail, verify_chk }
}

impl MainWindow {
    pub(super) fn wire_monitors_events(self: &Rc<Self>) {
        let this = Rc::clone(self);
        self.mon_detect_btn.on().bn_clicked(move || {
            this.refresh_monitors();
            Ok(())
        });

        let this = Rc::clone(self);
        self.mon_identify_btn.on().bn_clicked(move || {
            let geometries = enumerate_monitor_geometry();
            if geometries.is_empty() {
                this.shared.controller.push_log("Identify Monitors: no monitors reported by Win32.");
            } else {
                super::identify_overlay::show(&geometries);
            }
            Ok(())
        });

        let this = Rc::clone(self);
        self.mon_list.on().lbn_sel_change(move || {
            this.populate_monitor_detail();
            Ok(())
        });

        let this = Rc::clone(self);
        self.mon_verify_chk.on().bn_clicked(move || {
            if let Some(index) = super::listbox_selected_index(&this.mon_list) {
                let monitors = this.shared.monitors.borrow();
                if let Some(m) = monitors.get(index as usize) {
                    let key = m.key.clone();
                    let checked = this.mon_verify_chk.is_checked();
                    this.shared.controller.with_config_mut(|cfg| {
                        cfg.monitor_verify.insert(key, checked);
                    });
                    let _ = this.shared.controller.save_config();
                }
            }
            Ok(())
        });
    }

    pub fn refresh_monitors(self: &Rc<Self>) {
        let monitors = self.shared.controller.detect_monitors();
        self.mon_list.items().delete_all();
        let labels: Vec<String> = monitors
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let name = m.hardware_info.as_ref().and_then(|hw| hw.model_name.clone()).unwrap_or_else(|| m.description.clone());
                format!("Monitor {} - {name}", i + 1)
            })
            .collect();
        let _ = self.mon_list.items().add(&labels);
        *self.shared.monitors.borrow_mut() = monitors;
        if !labels.is_empty() {
            super::listbox_select(&self.mon_list, Some(0));
        }
        self.populate_monitor_detail();
        self.refresh_profile_monitor_rows();
    }

    fn populate_monitor_detail(self: &Rc<Self>) {
        let Some(index) = super::listbox_selected_index(&self.mon_list) else {
            let _ = self.mon_detail.set_text("");
            return;
        };
        let monitors = self.shared.monitors.borrow();
        let Some(m) = monitors.get(index as usize) else {
            return;
        };

        let mut text = String::new();
        text.push_str(&format!("Internal key: {}\r\n", m.key.0));
        text.push_str(&format!("OS description: {}\r\n", m.description));
        match &m.hardware_info {
            Some(hw) => {
                if let Some(v) = &hw.manufacturer {
                    text.push_str(&format!("Manufacturer code: {v}\r\n"));
                }
                if let Some(v) = &hw.model_name {
                    text.push_str(&format!("Model: {v}\r\n"));
                }
                if let Some(v) = &hw.serial {
                    text.push_str(&format!("Serial: {v}\r\n"));
                }
                match (hw.manufacture_week, hw.manufacture_year) {
                    (Some(w), Some(y)) => text.push_str(&format!("Manufactured: week {w}, {y}\r\n")),
                    (None, Some(y)) => text.push_str(&format!("Manufactured: {y}\r\n")),
                    _ => {}
                }
            }
            None => text.push_str("(no EDID info available for this monitor - Windows/WMI didn't report it)\r\n"),
        }
        text.push_str("\r\nReported input values (may not match reality - some monitors misreport this):\r\n");
        if m.input_capabilities.is_empty() {
            text.push_str("(none reported - use manual raw-hex input in Profiles)\r\n");
        } else {
            for cap in &m.input_capabilities {
                text.push_str(&format!("  {} = 0x{:02X}\r\n", cap.name, cap.code));
            }
        }
        let _ = self.mon_detail.set_text(&text);

        let verify = self.shared.controller.with_config_mut(|cfg| cfg.verify_enabled(&m.key));
        self.mon_verify_chk.set_check(verify);
    }
}
