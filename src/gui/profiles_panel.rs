use std::rc::Rc;

use uuid::Uuid;
use winsafe::gui;
use winsafe::prelude::*;

use super::{MainWindow, ProfileMonitorRow, HOTKEY_CAPTURE_TIMER_ID, MAX_MONITORS, PANEL_H, PANEL_W};
use crate::config::InputSourceValue;

pub struct ProfilesPanel {
    pub panel: gui::WindowControl,
    pub list: gui::ListBox,
    pub new_name: gui::Edit,
    pub add_btn: gui::Button,
    pub delete_btn: gui::Button,
    pub name_edit: gui::Edit,
    pub hotkey_btn: gui::Button,
    pub rows: Vec<ProfileMonitorRow>,
}

const ROW_TOP: i32 = 75;
const ROW_HEIGHT: i32 = 32;

pub fn build(parent: &(impl GuiParent + 'static)) -> ProfilesPanel {
    let panel = gui::WindowControl::new(
        parent,
        gui::WindowControlOpts {
            position: (0, 0),
            size: (PANEL_W, PANEL_H),
            ex_style: winsafe::co::WS_EX::LEFT | winsafe::co::WS_EX::CONTROLPARENT,
            ..Default::default()
        },
    );

    let list = gui::ListBox::new(&panel, gui::ListBoxOpts { position: (10, 10), size: (150, PANEL_H - 100), ..Default::default() });
    let new_name = gui::Edit::new(&panel, gui::EditOpts { position: (10, PANEL_H - 85), width: 100, ..Default::default() });
    let add_btn = gui::Button::new(&panel, gui::ButtonOpts { position: (115, PANEL_H - 86), text: "Add".to_owned(), width: 45, ..Default::default() });
    let delete_btn = gui::Button::new(
        &panel,
        gui::ButtonOpts { position: (10, PANEL_H - 55), text: "Delete Selected".to_owned(), width: 150, ..Default::default() },
    );

    let _name_label = gui::Label::new(&panel, gui::LabelOpts { position: (170, 12), text: "Name:".to_owned(), ..Default::default() });
    let name_edit = gui::Edit::new(&panel, gui::EditOpts { position: (220, 9), width: PANEL_W - 235, ..Default::default() });

    let _hotkey_label = gui::Label::new(&panel, gui::LabelOpts { position: (170, 42), text: "Hotkey:".to_owned(), ..Default::default() });
    let hotkey_btn = gui::Button::new(
        &panel,
        gui::ButtonOpts { position: (220, 39), text: "(none)".to_owned(), width: 200, ..Default::default() },
    );

    let mut rows = Vec::with_capacity(MAX_MONITORS);
    for i in 0..MAX_MONITORS {
        let y = ROW_TOP + (i as i32) * ROW_HEIGHT;
        let label = gui::Label::new(&panel, gui::LabelOpts { position: (170, y + 4), size: (140, 20), text: String::new(), ..Default::default() });
        let combo = gui::ComboBox::new(&panel, gui::ComboBoxOpts { position: (315, y), width: 140, ..Default::default() });
        let manual_chk = gui::CheckBox::new(
            &panel,
            gui::CheckBoxOpts { position: (465, y + 2), text: "Manual".to_owned(), size: (70, 20), ..Default::default() },
        );
        let hex_edit = gui::Edit::new(&panel, gui::EditOpts { position: (540, y), width: 60, ..Default::default() });
        rows.push(ProfileMonitorRow { label, combo, manual_chk, hex_edit });
    }

    ProfilesPanel { panel, list, new_name, add_btn, delete_btn, name_edit, hotkey_btn, rows }
}

impl MainWindow {
    pub(super) fn wire_profiles_events(self: &Rc<Self>) {
        let this = Rc::clone(self);
        self.prof_list.on().lbn_sel_change(move || {
            this.select_profile_from_list();
            Ok(())
        });

        let this = Rc::clone(self);
        self.prof_add_btn.on().bn_clicked(move || {
            let name = this.prof_new_name.text().unwrap_or_default();
            let name = name.trim();
            if !name.is_empty() {
                let id = this.shared.controller.add_profile(name);
                let _ = this.prof_new_name.set_text("");
                this.refresh_profiles_list();
                this.select_profile(Some(id));
                this.persist_profiles_change();
            }
            Ok(())
        });

        let this = Rc::clone(self);
        self.prof_delete_btn.on().bn_clicked(move || {
            if let Some(id) = this.selected_profile.get() {
                this.shared.controller.delete_profile(id);
                this.selected_profile.set(None);
                this.refresh_profiles_list();
                this.persist_profiles_change();
            }
            Ok(())
        });

        let this = Rc::clone(self);
        self.prof_name_edit.on().en_kill_focus(move || {
            this.commit_name_edit();
            Ok(())
        });

        let this = Rc::clone(self);
        self.prof_hotkey_btn.on().bn_clicked(move || {
            this.capturing_hotkey.set(true);
            let _ = this.prof_hotkey_btn.hwnd().SetWindowText("Press a key... (Esc to cancel)");
            let _ = this.wnd.hwnd().SetTimer(HOTKEY_CAPTURE_TIMER_ID, 50, None);
            Ok(())
        });

        let this = Rc::clone(self);
        self.wnd.on().wm_timer(HOTKEY_CAPTURE_TIMER_ID, move || {
            this.on_hotkey_capture_tick();
            Ok(())
        });

        for i in 0..MAX_MONITORS {
            let this = Rc::clone(self);
            self.prof_rows[i].combo.on().cbn_sel_change(move || {
                this.commit_row_combo(i);
                Ok(())
            });

            let this = Rc::clone(self);
            self.prof_rows[i].manual_chk.on().bn_clicked(move || {
                this.toggle_row_manual(i);
                Ok(())
            });

            let this = Rc::clone(self);
            self.prof_rows[i].hex_edit.on().en_kill_focus(move || {
                this.commit_row_hex(i);
                Ok(())
            });
        }
    }

    fn on_hotkey_capture_tick(self: &Rc<Self>) {
        if !self.capturing_hotkey.get() {
            let _ = self.wnd.hwnd().KillTimer(HOTKEY_CAPTURE_TIMER_ID);
            return;
        }
        let Some(binding) = super::hotkey_capture::poll() else {
            return;
        };
        self.capturing_hotkey.set(false);
        let _ = self.wnd.hwnd().KillTimer(HOTKEY_CAPTURE_TIMER_ID);

        if !super::hotkey_capture::is_cancel(&binding)
            && let Some(id) = self.selected_profile.get() {
                self.shared.controller.with_config_mut(|cfg| {
                    if let Some(p) = cfg.profiles.iter_mut().find(|p| p.id == id) {
                        p.hotkey = Some(binding);
                    }
                });
                self.persist_profiles_change();
            }
        self.populate_profile_editor();
    }

    pub fn refresh_profiles_list(self: &Rc<Self>) {
        let profiles = self.shared.controller.profiles_snapshot();
        self.prof_list.items().delete_all();
        let names: Vec<String> = profiles.iter().map(|p| p.name.clone()).collect();
        let _ = self.prof_list.items().add(&names);

        if let Some(selected) = self.selected_profile.get() {
            if let Some(idx) = profiles.iter().position(|p| p.id == selected) {
                super::listbox_select(&self.prof_list, Some(idx as u32));
            } else {
                self.selected_profile.set(None);
            }
        }
        self.populate_profile_editor();
    }

    fn select_profile_from_list(self: &Rc<Self>) {
        let profiles = self.shared.controller.profiles_snapshot();
        let id = super::listbox_selected_index(&self.prof_list)
            .and_then(|i| profiles.get(i as usize))
            .map(|p| p.id);
        self.select_profile(id);
    }

    fn select_profile(self: &Rc<Self>, id: Option<Uuid>) {
        self.selected_profile.set(id);
        self.populate_profile_editor();
    }

    fn populate_profile_editor(self: &Rc<Self>) {
        let Some(id) = self.selected_profile.get() else {
            let _ = self.prof_name_edit.set_text("");
            let _ = self.prof_hotkey_btn.hwnd().SetWindowText("(none)");
            self.refresh_profile_monitor_rows();
            return;
        };
        let profiles = self.shared.controller.profiles_snapshot();
        let Some(profile) = profiles.iter().find(|p| p.id == id) else {
            return;
        };
        let _ = self.prof_name_edit.set_text(&profile.name);
        let label = profile.hotkey.as_ref().map(|h| h.display.clone()).unwrap_or_else(|| "(none)".to_string());
        let _ = self.prof_hotkey_btn.hwnd().SetWindowText(&label);
        self.refresh_profile_monitor_rows();
    }

    fn commit_name_edit(self: &Rc<Self>) {
        let Some(id) = self.selected_profile.get() else { return };
        let name = self.prof_name_edit.text().unwrap_or_default();
        if name.trim().is_empty() {
            return;
        }
        self.shared.controller.with_config_mut(|cfg| {
            if let Some(p) = cfg.profiles.iter_mut().find(|p| p.id == id) {
                p.name = name.clone();
            }
        });
        self.refresh_profiles_list();
        self.persist_profiles_change();
    }

    /// Repopulates the fixed pool of per-monitor rows from the currently
    /// detected monitors and the selected profile's saved assignments -
    /// showing only as many rows as there are real monitors (up to
    /// `MAX_MONITORS`) and hiding the rest.
    pub fn refresh_profile_monitor_rows(self: &Rc<Self>) {
        let monitors = self.shared.monitors.borrow();
        let profile_id = self.selected_profile.get();
        let assignments = profile_id.and_then(|id| {
            self.shared
                .controller
                .profiles_snapshot()
                .into_iter()
                .find(|p| p.id == id)
                .map(|p| p.assignments)
        });

        for (i, row) in self.prof_rows.iter().enumerate() {
            let Some(m) = monitors.get(i) else {
                row.label.hwnd().ShowWindow(winsafe::co::SW::HIDE);
                row.combo.hwnd().ShowWindow(winsafe::co::SW::HIDE);
                row.manual_chk.hwnd().ShowWindow(winsafe::co::SW::HIDE);
                row.hex_edit.hwnd().ShowWindow(winsafe::co::SW::HIDE);
                continue;
            };

            let name = m.hardware_info.as_ref().and_then(|hw| hw.model_name.clone()).unwrap_or_else(|| m.description.clone());
            let _ = row.label.set_text_and_resize(&format!("Monitor {}: {name}", i + 1));
            row.label.hwnd().ShowWindow(winsafe::co::SW::SHOW);

            let current = assignments.as_ref().and_then(|a| a.get(&m.key).cloned());
            let has_capabilities = !m.input_capabilities.is_empty();
            let manual_mode = !has_capabilities || matches!(current, Some(InputSourceValue::RawVcp(_)));

            row.combo.items().delete_all();
            for cap in &m.input_capabilities {
                let _ = row.combo.items().add(&[format!("{} (0x{:02X})", cap.name, cap.code)]);
            }
            if let Some(InputSourceValue::Named(name, code)) = &current
                && let Some(pos) = m.input_capabilities.iter().position(|c| c.name == *name && c.code == *code) {
                    super::combo_select(&row.combo, Some(pos as u32));
                }

            row.manual_chk.set_check(manual_mode);
            let hex_text = match &current {
                Some(v) => format!("{:02X}", v.vcp_code()),
                None => String::new(),
            };
            let _ = row.hex_edit.set_text(&hex_text);

            if has_capabilities {
                row.manual_chk.hwnd().ShowWindow(winsafe::co::SW::SHOW);
                row.combo.hwnd().ShowWindow(if manual_mode { winsafe::co::SW::HIDE } else { winsafe::co::SW::SHOW });
                row.hex_edit.hwnd().ShowWindow(if manual_mode { winsafe::co::SW::SHOW } else { winsafe::co::SW::HIDE });
            } else {
                row.manual_chk.hwnd().ShowWindow(winsafe::co::SW::HIDE);
                row.combo.hwnd().ShowWindow(winsafe::co::SW::HIDE);
                row.hex_edit.hwnd().ShowWindow(winsafe::co::SW::SHOW);
            }
        }
    }

    fn commit_row_combo(self: &Rc<Self>, row_index: usize) {
        let Some(profile_id) = self.selected_profile.get() else { return };
        let monitors = self.shared.monitors.borrow();
        let Some(m) = monitors.get(row_index) else { return };
        let row = &self.prof_rows[row_index];
        let Some(sel) = super::combo_selected_index(&row.combo) else { return };
        let Some(cap) = m.input_capabilities.get(sel as usize) else { return };
        let key = m.key.clone();
        let value = InputSourceValue::Named(cap.name.clone(), cap.code);
        drop(monitors);
        self.shared.controller.with_config_mut(|cfg| {
            if let Some(p) = cfg.profiles.iter_mut().find(|p| p.id == profile_id) {
                p.assignments.insert(key, value);
            }
        });
        self.persist_profiles_change();
    }

    fn toggle_row_manual(self: &Rc<Self>, row_index: usize) {
        self.refresh_profile_monitor_rows();
        let _ = row_index;
    }

    fn commit_row_hex(self: &Rc<Self>, row_index: usize) {
        let Some(profile_id) = self.selected_profile.get() else { return };
        let monitors = self.shared.monitors.borrow();
        let Some(m) = monitors.get(row_index) else { return };
        let key = m.key.clone();
        drop(monitors);
        let row = &self.prof_rows[row_index];
        let text = row.hex_edit.text().unwrap_or_default();
        let Ok(code) = u8::from_str_radix(text.trim().trim_start_matches("0x"), 16) else {
            return;
        };
        self.shared.controller.with_config_mut(|cfg| {
            if let Some(p) = cfg.profiles.iter_mut().find(|p| p.id == profile_id) {
                p.assignments.insert(key, InputSourceValue::RawVcp(code));
            }
        });
        self.persist_profiles_change();
    }

    /// Re-syncs global hotkeys (and the `Send`-safe lookup map the hotkey
    /// callback actually reads), rebuilds the tray quick-switch menu, and
    /// saves the config - call after any profile/hotkey/assignment edit.
    pub(super) fn persist_profiles_change(self: &Rc<Self>) {
        let profiles = self.shared.controller.profiles_snapshot();
        let mut hotkeys = self.shared.hotkeys.borrow_mut();
        hotkeys.sync(&profiles);
        *self.shared.hotkey_lookup.lock().unwrap() = hotkeys.lookup_snapshot();
        drop(hotkeys);
        crate::tray::rebuild_menu(&mut self.shared.tray.borrow_mut(), &profiles);
        let _ = self.shared.controller.save_config();
    }
}
