use std::collections::HashMap;

use orchid_fs::{shell_icon, FsPath, ShellIconSize};
use orchid_i18n::LocaleManager;
use orchid_widgets::ProcessesPayload;
use slint::{Image, Model, ModelRc, SharedString, VecModel};

use crate::slint_generated::{
    ProcessRowEntry, ProcessesConfirmDialog, ProcessesModel, ServiceRowEntry, StartupRowEntry,
    UserRowEntry,
};

fn sync_rows<T: Clone + 'static>(model: &ModelRc<T>, new_rows: Vec<T>) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<T>>() else {
        return;
    };
    while v.row_count() > new_rows.len() {
        v.remove(v.row_count() - 1);
    }
    for (i, row) in new_rows.into_iter().enumerate() {
        if i < v.row_count() {
            v.set_row_data(i, row);
        } else {
            v.push(row);
        }
    }
}

fn sync_process_rows(model: &ModelRc<ProcessRowEntry>, new_rows: Vec<ProcessRowEntry>) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<ProcessRowEntry>>() else {
        return;
    };
    while v.row_count() > new_rows.len() {
        v.remove(v.row_count() - 1);
    }
    for (i, row) in new_rows.into_iter().enumerate() {
        if i < v.row_count() {
            if let Some(old) = v.row_data(i) {
                if process_row_eq(&old, &row) {
                    continue;
                }
            }
            v.set_row_data(i, row);
        } else {
            v.push(row);
        }
    }
}

fn process_row_eq(a: &ProcessRowEntry, b: &ProcessRowEntry) -> bool {
    a.pid == b.pid
        && a.pid_text == b.pid_text
        && a.name == b.name
        && a.status == b.status
        && a.cpu_text == b.cpu_text
        && a.memory_text == b.memory_text
        && a.io_text == b.io_text
        && a.user == b.user
        && a.path == b.path
        && a.is_group_header == b.is_group_header
        && a.group_label == b.group_label
        && a.has_icon == b.has_icon
}

pub(crate) fn empty_processes_confirm() -> ProcessesConfirmDialog {
    ProcessesConfirmDialog {
        visible: false,
        title: SharedString::new(),
        message: SharedString::new(),
        confirm_label: SharedString::new(),
        cancel_label: SharedString::new(),
        pending_action: SharedString::new(),
    }
}

pub(crate) fn empty_processes_model(locale: &LocaleManager) -> ProcessesModel {
    base_model(
        locale,
        &ProcessesPayload {
            tab: orchid_widgets::ProcessesTab::Processes,
            search_query: String::new(),
            sort_column: orchid_widgets::ProcessSortColumn::Cpu,
            sort_descending: true,
            selected_pid: 0,
            selected_service: String::new(),
            selected_startup: String::new(),
            selected_session: u32::MAX,
            processes: Vec::new(),
            services: Vec::new(),
            startups: Vec::new(),
            users: Vec::new(),
            is_loading: true,
            status_message: String::new(),
            show_grouping: true,
        },
        false,
        0.0,
        0.0,
        empty_processes_confirm(),
        false,
    )
}

pub(crate) fn build_processes_model(
    p: &ProcessesPayload,
    locale: &LocaleManager,
    context_visible: bool,
    context_x: f32,
    context_y: f32,
    confirm_dialog: ProcessesConfirmDialog,
) -> ProcessesModel {
    base_model(
        locale,
        p,
        context_visible,
        context_x,
        context_y,
        confirm_dialog,
        true,
    )
}

/// Result of an in-place processes model patch.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProcessesPatchResult {
    /// True when parent [`ProcessesModel`] scalars changed and the frame row
    /// must be written back via `set_row_data`.
    pub needs_frame_write: bool,
}

/// Update an existing [`ProcessesModel`] in place (keep the same `ModelRc`s).
///
/// Periodic refresh must not replace `ModelRc` — Slint would tear down and
/// recreate every `for` row. List updates notify through the shared `VecModel`.
pub(crate) fn patch_processes_model(
    model: &mut ProcessesModel,
    p: &ProcessesPayload,
    _locale: &LocaleManager,
    context_visible: bool,
    context_x: f32,
    context_y: f32,
    confirm_dialog: ProcessesConfirmDialog,
) -> ProcessesPatchResult {
    let needs_frame_write = apply_dynamic_scalars(
        model,
        p,
        context_visible,
        context_x,
        context_y,
        confirm_dialog,
    );

    let mut rows = process_row_entries(p, false);
    preserve_process_icons(&model.processes, &mut rows);
    sync_process_rows(&model.processes, rows);
    sync_rows(&model.services, service_row_entries(p));
    sync_rows(&model.startups, startup_row_entries(p));
    sync_rows(&model.users, user_row_entries(p));
    ProcessesPatchResult { needs_frame_write }
}

fn base_model(
    locale: &LocaleManager,
    p: &ProcessesPayload,
    context_visible: bool,
    context_x: f32,
    context_y: f32,
    confirm_dialog: ProcessesConfirmDialog,
    fetch_icons: bool,
) -> ProcessesModel {
    let mut model = ProcessesModel {
        tab: 0,
        search_query: SharedString::new(),
        search_placeholder: SharedString::new(),
        sort_column: 0,
        sort_descending: false,
        selected_pid: 0,
        selected_service: SharedString::new(),
        selected_startup: SharedString::new(),
        selected_session: -1,
        processes: ModelRc::new(VecModel::default()),
        services: ModelRc::new(VecModel::default()),
        startups: ModelRc::new(VecModel::default()),
        users: ModelRc::new(VecModel::default()),
        status_message: SharedString::new(),
        is_loading: false,
        tab_processes_label: SharedString::new(),
        tab_services_label: SharedString::new(),
        tab_startup_label: SharedString::new(),
        tab_users_label: SharedString::new(),
        col_name_label: SharedString::new(),
        col_pid_label: SharedString::new(),
        col_cpu_label: SharedString::new(),
        col_memory_label: SharedString::new(),
        col_io_label: SharedString::new(),
        col_user_label: SharedString::new(),
        end_task_label: SharedString::new(),
        end_tree_label: SharedString::new(),
        open_location_label: SharedString::new(),
        copy_pid_label: SharedString::new(),
        copy_path_label: SharedString::new(),
        service_start_label: SharedString::new(),
        service_stop_label: SharedString::new(),
        service_restart_label: SharedString::new(),
        user_disconnect_label: SharedString::new(),
        user_sign_out_label: SharedString::new(),
        context_visible: false,
        context_x: 0.0,
        context_y: 0.0,
        confirm_dialog: empty_processes_confirm(),
    };
    apply_labels(&mut model, locale);
    apply_dynamic_scalars(
        &mut model,
        p,
        context_visible,
        context_x,
        context_y,
        confirm_dialog,
    );
    sync_process_rows(&model.processes, process_row_entries(p, fetch_icons));
    sync_rows(&model.services, service_row_entries(p));
    sync_rows(&model.startups, startup_row_entries(p));
    sync_rows(&model.users, user_row_entries(p));
    model
}

fn apply_labels(model: &mut ProcessesModel, locale: &LocaleManager) {
    model.search_placeholder = locale.tr("processes-search-placeholder").into();
    model.tab_processes_label = locale.tr("processes-tab-processes").into();
    model.tab_services_label = locale.tr("processes-tab-services").into();
    model.tab_startup_label = locale.tr("processes-tab-startup").into();
    model.tab_users_label = locale.tr("processes-tab-users").into();
    model.col_name_label = locale.tr("processes-col-name").into();
    model.col_pid_label = locale.tr("processes-col-pid").into();
    model.col_cpu_label = locale.tr("processes-col-cpu").into();
    model.col_memory_label = locale.tr("processes-col-memory").into();
    model.col_io_label = locale.tr("processes-col-io").into();
    model.col_user_label = locale.tr("processes-col-user").into();
    model.end_task_label = locale.tr("processes-end-task").into();
    model.end_tree_label = locale.tr("processes-end-tree").into();
    model.open_location_label = locale.tr("processes-open-location").into();
    model.copy_pid_label = locale.tr("processes-copy-pid").into();
    model.copy_path_label = locale.tr("processes-copy-path").into();
    model.service_start_label = locale.tr("processes-service-start").into();
    model.service_stop_label = locale.tr("processes-service-stop").into();
    model.service_restart_label = locale.tr("processes-service-restart").into();
    model.user_disconnect_label = locale.tr("processes-user-disconnect").into();
    model.user_sign_out_label = locale.tr("processes-user-sign-out").into();
}

/// Apply selection / status / context fields. Returns whether anything changed.
fn apply_dynamic_scalars(
    model: &mut ProcessesModel,
    p: &ProcessesPayload,
    context_visible: bool,
    context_x: f32,
    context_y: f32,
    confirm_dialog: ProcessesConfirmDialog,
) -> bool {
    let selected_session = if p.selected_session == u32::MAX {
        -1
    } else {
        p.selected_session as i32
    };
    let mut changed = false;
    macro_rules! set_if {
        ($field:ident, $val:expr) => {
            if model.$field != $val {
                model.$field = $val;
                changed = true;
            }
        };
    }
    set_if!(tab, p.tab.as_index());
    let search: SharedString = p.search_query.clone().into();
    set_if!(search_query, search);
    set_if!(sort_column, p.sort_column.as_index());
    set_if!(sort_descending, p.sort_descending);
    set_if!(selected_pid, p.selected_pid as i32);
    let selected_service: SharedString = p.selected_service.clone().into();
    set_if!(selected_service, selected_service);
    let selected_startup: SharedString = p.selected_startup.clone().into();
    set_if!(selected_startup, selected_startup);
    set_if!(selected_session, selected_session);
    let status: SharedString = p.status_message.clone().into();
    set_if!(status_message, status);
    set_if!(is_loading, p.is_loading);
    set_if!(context_visible, context_visible);
    if (model.context_x - context_x).abs() > f32::EPSILON
        || (model.context_y - context_y).abs() > f32::EPSILON
    {
        model.context_x = context_x;
        model.context_y = context_y;
        changed = true;
    }
    if model.confirm_dialog.visible != confirm_dialog.visible
        || model.confirm_dialog.title != confirm_dialog.title
        || model.confirm_dialog.message != confirm_dialog.message
        || model.confirm_dialog.confirm_label != confirm_dialog.confirm_label
        || model.confirm_dialog.cancel_label != confirm_dialog.cancel_label
        || model.confirm_dialog.pending_action != confirm_dialog.pending_action
    {
        model.confirm_dialog = confirm_dialog;
        changed = true;
    }
    changed
}

fn process_row_entries(p: &ProcessesPayload, fetch_icons: bool) -> Vec<ProcessRowEntry> {
    // Shell icons for .exe are expensive; only on first/full build, and only a few rows.
    const MAX_PROCESS_ICONS: usize = 24;
    let mut icons_left = if fetch_icons { MAX_PROCESS_ICONS } else { 0 };
    p.processes
        .iter()
        .map(|r| {
            let (icon, has_icon) = if r.is_group_header || r.path.is_empty() {
                (Image::default(), false)
            } else if icons_left > 0 {
                icons_left -= 1;
                process_icon(&r.path)
            } else {
                (Image::default(), false)
            };
            ProcessRowEntry {
                pid: r.pid as i32,
                pid_text: r.pid.to_string().into(),
                name: r.name.clone().into(),
                status: r.status.clone().into(),
                cpu_text: format!("{:.1}", r.cpu_percent).into(),
                memory_text: r.memory_text.clone().into(),
                io_text: r.io_text.clone().into(),
                user: r.user.clone().into(),
                path: r.path.clone().into(),
                is_group_header: r.is_group_header,
                group_label: r.group_label.clone().into(),
                icon,
                has_icon,
            }
        })
        .collect()
}

fn preserve_process_icons(model: &ModelRc<ProcessRowEntry>, rows: &mut [ProcessRowEntry]) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<ProcessRowEntry>>() else {
        return;
    };
    let mut by_pid: HashMap<i32, Image> = HashMap::new();
    for i in 0..v.row_count() {
        if let Some(old) = v.row_data(i) {
            if old.has_icon && !old.is_group_header {
                by_pid.insert(old.pid, old.icon);
            }
        }
    }
    for row in rows.iter_mut() {
        if row.is_group_header || row.has_icon {
            continue;
        }
        if let Some(icon) = by_pid.remove(&row.pid) {
            row.icon = icon;
            row.has_icon = true;
        }
    }
}

fn service_row_entries(p: &ProcessesPayload) -> Vec<ServiceRowEntry> {
    p.services
        .iter()
        .map(|r| ServiceRowEntry {
            name: r.name.clone().into(),
            display_name: r.display_name.clone().into(),
            status: r.status.clone().into(),
            start_type: r.start_type.clone().into(),
            pid_text: if r.pid == 0 {
                SharedString::new()
            } else {
                r.pid.to_string().into()
            },
        })
        .collect()
}

fn startup_row_entries(p: &ProcessesPayload) -> Vec<StartupRowEntry> {
    p.startups
        .iter()
        .map(|r| StartupRowEntry {
            id: r.id.clone().into(),
            name: r.name.clone().into(),
            command: r.command.clone().into(),
            location: r.location.clone().into(),
            enabled: r.enabled,
            can_toggle: r.can_toggle,
        })
        .collect()
}

fn user_row_entries(p: &ProcessesPayload) -> Vec<UserRowEntry> {
    p.users
        .iter()
        .map(|r| UserRowEntry {
            session_id: r.session_id as i32,
            session_text: r.session_id.to_string().into(),
            user_name: r.user_name.clone().into(),
            state: r.state.clone().into(),
            process_count_text: r.process_count.to_string().into(),
            memory_text: r.memory_text.clone().into(),
        })
        .collect()
}

fn process_icon(path: &str) -> (Image, bool) {
    let Ok(fs_path) = FsPath::from_local(std::path::Path::new(path)) else {
        return (Image::default(), false);
    };
    let Some(icon) = shell_icon(&fs_path, false, ShellIconSize::Small) else {
        return (Image::default(), false);
    };
    if icon.width == 0 || icon.height == 0 || icon.rgba.is_empty() {
        return (Image::default(), false);
    }
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        icon.rgba.as_slice(),
        icon.width,
        icon.height,
    );
    (Image::from_rgba8(buf), true)
}
