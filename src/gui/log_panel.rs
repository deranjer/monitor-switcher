use winsafe::gui;
use winsafe::prelude::*;

use super::{PANEL_H, PANEL_W};

pub struct LogPanel {
    pub panel: gui::WindowControl,
    pub edit: gui::Edit,
}

pub fn build(parent: &(impl GuiParent + 'static)) -> LogPanel {
    let panel = gui::WindowControl::new(
        parent,
        gui::WindowControlOpts {
            position: (0, 0),
            size: (PANEL_W, PANEL_H),
            ex_style: winsafe::co::WS_EX::LEFT | winsafe::co::WS_EX::CONTROLPARENT,
            ..Default::default()
        },
    );

    let edit = gui::Edit::new(
        &panel,
        gui::EditOpts {
            position: (10, 10),
            width: PANEL_W - 20,
            height: PANEL_H - 20,
            control_style: winsafe::co::ES::MULTILINE | winsafe::co::ES::AUTOVSCROLL | winsafe::co::ES::READONLY,
            window_style: winsafe::co::WS::CHILD | winsafe::co::WS::VISIBLE | winsafe::co::WS::VSCROLL | winsafe::co::WS::TABSTOP,
            ..Default::default()
        },
    );

    LogPanel { panel, edit }
}
