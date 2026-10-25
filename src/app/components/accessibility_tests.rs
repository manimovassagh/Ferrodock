//! What a screen reader finds in the hand-painted widgets: walks the
//! AccessKit tree egui builds, without a window or GPU.

use eframe::egui;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};

use super::{palette, table, toast};
use crate::theme::DARK;

#[test]
fn table_rows_have_a_name_and_a_selection() {
    let harness = Harness::new_ui(|ui| {
        table::show(
            ui,
            &DARK,
            "test",
            vec![table::Col::fill("NAME")],
            2,
            |i| i == 1,
            |i| format!("Container web-{i}, Up 2 minutes"),
            |i, row| {
                row.col(|ui| {
                    ui.label(format!("web-{i}"));
                });
            },
        );
    });
    let first = harness.get_by_role_and_label(Role::Row, "Container web-0, Up 2 minutes");
    let second = harness.get_by_role_and_label(Role::Row, "Container web-1, Up 2 minutes");
    assert_eq!(first.accesskit_node().is_selected(), Some(false));
    assert_eq!(second.accesskit_node().is_selected(), Some(true));
}

#[test]
fn palette_rows_are_options() {
    let entry = palette::Entry {
        label: "web-1".into(),
        detail: "Container · running".into(),
    };
    let harness = Harness::new_ui(|ui| {
        palette::row(ui, &DARK, &entry, true);
    });
    let option = harness.get_by_role_and_label(Role::ListBoxOption, "web-1, Container · running");
    assert_eq!(option.accesskit_node().is_selected(), Some(true));
}

#[test]
fn toasts_are_announced() {
    for (text, is_error, role) in [
        ("Couldn't remove web-1", true, Role::Alert),
        ("Removed web-2", false, Role::Status),
    ] {
        let toast = toast::Toast::new(text.into(), is_error);
        // Toasts draw an icon glyph the test fonts lack; only the tree matters.
        let harness = Harness::builder().allow_missing_glyphs().build_ui(|ui| {
            toast.show(ui, &DARK);
        });
        harness.get_by(|node| node.role() == role && node.value().as_deref() == Some(text));
    }
}
