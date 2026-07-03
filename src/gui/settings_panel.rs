use std::rc::Rc;

use winsafe::gui;
use winsafe::prelude::*;

use super::{MainWindow, PANEL_W};
use crate::platform::windows::autostart;

pub struct SettingsPanel {
    pub panel: gui::WindowControl,
    pub autostart_chk: gui::CheckBox,
}

pub fn build(parent: &(impl GuiParent + 'static)) -> SettingsPanel {
    let panel = gui::WindowControl::new(
        parent,
        gui::WindowControlOpts {
            position: (0, 0),
            size: (PANEL_W, super::PANEL_H),
            ex_style: winsafe::co::WS_EX::LEFT | winsafe::co::WS_EX::CONTROLPARENT,
            ..Default::default()
        },
    );

    let autostart_chk = gui::CheckBox::new(
        &panel,
        gui::CheckBoxOpts {
            position: (10, 10),
            text: "Start automatically at Windows login".to_owned(),
            size: (PANEL_W - 20, 20),
            ..Default::default()
        },
    );

    SettingsPanel { panel, autostart_chk }
}

impl MainWindow {
    pub(super) fn wire_settings_events(self: &Rc<Self>) {
        let this = Rc::clone(self);
        self.settings_autostart_chk.on().bn_clicked(move || {
            let enabled = this.settings_autostart_chk.is_checked();
            let exe_path = std::env::current_exe().ok().and_then(|p| p.to_str().map(str::to_string));
            let result: anyhow::Result<()> = if enabled {
                exe_path
                    .ok_or_else(|| anyhow::anyhow!("could not resolve exe path"))
                    .and_then(|p| autostart::enable(&p).map_err(|e| anyhow::anyhow!("{e}")))
            } else {
                autostart::disable().map_err(|e| anyhow::anyhow!("{e}"))
            };
            if let Err(e) = result {
                tracing::warn!("failed to update autostart registration: {e}");
            }
            this.shared.controller.with_config_mut(|cfg| cfg.autostart_enabled = autostart::is_enabled());
            let _ = this.shared.controller.save_config();
            Ok(())
        });
    }
}
