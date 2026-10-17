//! The container list.

use eframe::egui;

use super::{ContainersPage, actions, ports, status};
use crate::app::components::table;
use crate::app::page::{PageCtx, toggle_selection};
use crate::docker::ContainerInfo;

pub(super) fn show(
    page: &mut ContainersPage,
    ui: &mut egui::Ui,
    ctx: &mut PageCtx,
    visible: &[&ContainerInfo],
) {
    let p = ctx.palette;
    let store = ctx.store;

    // Drop the less important columns when space is tight (e.g. with the
    // details pane open); both are still shown in the details pane.
    let width = ui.available_width();
    let show_ports = width >= 700.0;
    let show_id = width >= 560.0;
    let mut columns = vec![table::Col::fill("NAME"), table::Col::fixed("STATUS", 150.0)];
    if show_id {
        columns.push(table::Col::fixed("ID", 110.0));
    }
    if show_ports {
        columns.push(table::Col::fixed("PORTS", 140.0));
    }
    columns.push(table::Col::fixed("", 100.0));

    let selected = page.selected.clone();
    let clicked = table::show(
        ui,
        p,
        "containers",
        columns,
        visible.len(),
        |i| selected.as_deref() == Some(visible[i].id.as_str()),
        |i, row| {
            let c = visible[i];
            row.col(|ui| table::two_line(ui, p, &c.name, &c.image));
            row.col(|ui| status::cell(ui, p, c));
            if show_id {
                row.col(|ui| table::mono(ui, p, c.short_id()));
            }
            if show_ports {
                row.col(|ui| ports::cell(ui, p, &c.ports));
            }
            row.col(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let busy = store.pending.contains(&c.id);
                if let Some(action) = actions::buttons(ui, c, busy, false) {
                    ctx.request(actions::intent(action, c));
                }
            });
        },
    );
    if let Some(i) = clicked {
        toggle_selection(&mut page.selected, visible[i].id.clone());
    }
}
