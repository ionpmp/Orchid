use orchid_i18n::LocaleManager;
use orchid_widgets::ProcessesPayload;
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{
    ProcessRowEntry, ProcessesModel, ServiceRowEntry, StartupRowEntry, UserRowEntry,
};

pub(crate) fn empty_processes_model(locale: &LocaleManager) -> ProcessesModel {
    base_model(locale, &ProcessesPayload {
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
    }, false, 0.0, 0.0)
}

pub(crate) fn build_processes_model(
    p: &ProcessesPayload,
    locale: &LocaleManager,
    context_visible: bool,
    context_x: f32,
    context_y: f32,
) -> ProcessesModel {
    base_model(locale, p, context_visible, context_x, context_y)
}

fn base_model(
    locale: &LocaleManager,
    p: &ProcessesPayload,
    context_visible: bool,
    context_x: f32,
    context_y: f32,
) -> ProcessesModel {
    let processes: Vec<ProcessRowEntry> = p
        .processes
        .iter()
        .map(|r| ProcessRowEntry {
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
        })
        .collect();
    let services: Vec<ServiceRowEntry> = p
        .services
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
        .collect();
    let startups: Vec<StartupRowEntry> = p
        .startups
        .iter()
        .map(|r| StartupRowEntry {
            id: r.id.clone().into(),
            name: r.name.clone().into(),
            command: r.command.clone().into(),
            location: r.location.clone().into(),
            enabled: r.enabled,
            can_toggle: r.can_toggle,
        })
        .collect();
    let users: Vec<UserRowEntry> = p
        .users
        .iter()
        .map(|r| UserRowEntry {
            session_id: r.session_id as i32,
            session_text: r.session_id.to_string().into(),
            user_name: r.user_name.clone().into(),
            state: r.state.clone().into(),
            process_count_text: r.process_count.to_string().into(),
            memory_text: r.memory_text.clone().into(),
        })
        .collect();

    ProcessesModel {
        tab: p.tab.as_index(),
        search_query: p.search_query.clone().into(),
        search_placeholder: locale.tr("processes-search-placeholder").into(),
        sort_column: p.sort_column.as_index(),
        sort_descending: p.sort_descending,
        selected_pid: p.selected_pid as i32,
        selected_service: p.selected_service.clone().into(),
        selected_startup: p.selected_startup.clone().into(),
        selected_session: if p.selected_session == u32::MAX {
            -1
        } else {
            p.selected_session as i32
        },
        processes: ModelRc::new(VecModel::from(processes)),
        services: ModelRc::new(VecModel::from(services)),
        startups: ModelRc::new(VecModel::from(startups)),
        users: ModelRc::new(VecModel::from(users)),
        status_message: p.status_message.clone().into(),
        is_loading: p.is_loading,
        tab_processes_label: locale.tr("processes-tab-processes").into(),
        tab_services_label: locale.tr("processes-tab-services").into(),
        tab_startup_label: locale.tr("processes-tab-startup").into(),
        tab_users_label: locale.tr("processes-tab-users").into(),
        col_name_label: locale.tr("processes-col-name").into(),
        col_pid_label: locale.tr("processes-col-pid").into(),
        col_cpu_label: locale.tr("processes-col-cpu").into(),
        col_memory_label: locale.tr("processes-col-memory").into(),
        col_io_label: locale.tr("processes-col-io").into(),
        col_user_label: locale.tr("processes-col-user").into(),
        end_task_label: locale.tr("processes-end-task").into(),
        end_tree_label: locale.tr("processes-end-tree").into(),
        open_location_label: locale.tr("processes-open-location").into(),
        copy_pid_label: locale.tr("processes-copy-pid").into(),
        copy_path_label: locale.tr("processes-copy-path").into(),
        service_start_label: locale.tr("processes-service-start").into(),
        service_stop_label: locale.tr("processes-service-stop").into(),
        service_restart_label: locale.tr("processes-service-restart").into(),
        user_disconnect_label: locale.tr("processes-user-disconnect").into(),
        user_sign_out_label: locale.tr("processes-user-sign-out").into(),
        context_visible,
        context_x,
        context_y,
    }
}
