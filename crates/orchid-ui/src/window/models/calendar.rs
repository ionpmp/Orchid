use orchid_i18n::LocaleManager;
use orchid_widgets::builtin::calendar::{format_minutes, parse_date};
use orchid_widgets::CalendarPayload;
use slint::{ModelRc, VecModel};

use crate::slint_generated::{
    CalendarDayEntry, CalendarEventEntry, CalendarModel, CalendarUpcomingEntry,
    CalendarWeekdayEntry,
};

pub(crate) fn empty_calendar_model(locale: &LocaleManager) -> CalendarModel {
    base_model(
        locale,
        &CalendarPayload {
            year: 1970,
            month: 1,
            selected_date: String::new(),
            today_date: String::new(),
            first_day_of_week: 1,
            days: Vec::new(),
            events: Vec::new(),
            upcoming: Vec::new(),
            show_upcoming: true,
            show_notes_preview: true,
            time_step_minutes: 15,
            color_filter: -1,
            editor_open: false,
            editor_event_id: String::new(),
            editor_is_new: true,
            editor_title: String::new(),
            editor_date: String::new(),
            editor_all_day: true,
            editor_start_hour: 9,
            editor_start_min: 0,
            editor_end_hour: 10,
            editor_end_min: 0,
            editor_notes: String::new(),
            editor_color: 0,
            delete_confirm_open: false,
        },
    )
}

pub(crate) fn build_calendar_model(p: &CalendarPayload, locale: &LocaleManager) -> CalendarModel {
    base_model(locale, p)
}

fn base_model(locale: &LocaleManager, p: &CalendarPayload) -> CalendarModel {
    let weekdays = weekday_labels(locale, p.first_day_of_week);
    let days: Vec<CalendarDayEntry> = p
        .days
        .iter()
        .map(|d| CalendarDayEntry {
            date_key: d.date_key.clone().into(),
            day: d.day,
            in_month: d.in_month,
            is_today: d.is_today,
            is_selected: d.is_selected,
            dot_colors: ModelRc::new(VecModel::from(
                d.dot_colors.iter().copied().collect::<Vec<i32>>(),
            )),
            event_count: d.event_count,
        })
        .collect();
    let events: Vec<CalendarEventEntry> = p
        .events
        .iter()
        .map(|e| CalendarEventEntry {
            id: e.id.clone().into(),
            title: e.title.clone().into(),
            time_label: e.time_label.clone().into(),
            notes_preview: e.notes_preview.clone().into(),
            all_day: e.all_day,
            color: e.color,
        })
        .collect();
    let upcoming: Vec<CalendarUpcomingEntry> = p
        .upcoming
        .iter()
        .map(|u| CalendarUpcomingEntry {
            id: u.id.clone().into(),
            title: u.title.clone().into(),
            date_key: u.date_key.clone().into(),
            date_label: selected_day_label(locale, &u.date_key).into(),
            time_label: u.time_label.clone().into(),
            all_day: u.all_day,
            color: u.color,
        })
        .collect();

    let month_title = locale.tr_args(
        "calendar-month-title",
        &orchid_i18n::FluentArgs::new()
            .with("month", month_name(locale, p.month))
            .with("year", p.year.to_string()),
    );
    let selected_label = selected_day_label(locale, &p.selected_date);
    let editor_date_label = selected_day_label(locale, &p.editor_date);

    CalendarModel {
        year: p.year,
        month: p.month,
        selected_date: p.selected_date.clone().into(),
        today_date: p.today_date.clone().into(),
        first_day_of_week: p.first_day_of_week,
        month_title: month_title.into(),
        selected_label: selected_label.into(),
        weekdays: ModelRc::new(VecModel::from(weekdays)),
        days: ModelRc::new(VecModel::from(days)),
        events: ModelRc::new(VecModel::from(events)),
        upcoming: ModelRc::new(VecModel::from(upcoming)),
        show_upcoming: p.show_upcoming,
        show_notes_preview: p.show_notes_preview,
        time_step_minutes: p.time_step_minutes,
        color_filter: p.color_filter,
        filter_all_label: locale.tr("calendar-filter-all").into(),
        filter_title: locale.tr("calendar-filter-title").into(),
        upcoming_title: locale.tr("calendar-upcoming").into(),
        events_count_label: locale
            .tr_args(
                "calendar-events-count",
                &orchid_i18n::FluentArgs::new().with("count", p.events.len().to_string()),
            )
            .into(),
        duplicate_label: locale.tr("calendar-duplicate").into(),
        color_options: ModelRc::new(VecModel::from(vec![0, 1, 2, 3, 4, 5])),
        editor_open: p.editor_open,
        editor_event_id: p.editor_event_id.clone().into(),
        editor_is_new: p.editor_is_new,
        editor_title: p.editor_title.clone().into(),
        editor_date: p.editor_date.clone().into(),
        editor_all_day: p.editor_all_day,
        editor_start_hour: p.editor_start_hour,
        editor_start_min: p.editor_start_min,
        editor_end_hour: p.editor_end_hour,
        editor_end_min: p.editor_end_min,
        editor_start_text: format_hm(p.editor_start_hour, p.editor_start_min).into(),
        editor_end_text: format_hm(p.editor_end_hour, p.editor_end_min).into(),
        editor_notes: p.editor_notes.clone().into(),
        editor_color: p.editor_color,
        editor_date_label: editor_date_label.into(),
        delete_confirm_open: p.delete_confirm_open,
        today_label: locale.tr("calendar-today").into(),
        all_day_label: locale.tr("calendar-all-day").into(),
        empty_label: locale.tr("calendar-empty-day").into(),
        untitled_label: locale.tr("calendar-untitled").into(),
        add_label: locale.tr("calendar-add").into(),
        edit_label: locale.tr("calendar-edit").into(),
        save_label: locale.tr("calendar-save").into(),
        delete_label: locale.tr("calendar-delete").into(),
        cancel_label: locale.tr("calendar-cancel").into(),
        delete_confirm_title: locale.tr("calendar-delete-confirm").into(),
        delete_confirm_yes: locale.tr("calendar-delete-confirm-yes").into(),
        title_field_label: locale.tr("calendar-field-title").into(),
        date_field_label: locale.tr("calendar-field-date").into(),
        notes_field_label: locale.tr("calendar-field-notes").into(),
        start_label: locale.tr("calendar-start").into(),
        end_label: locale.tr("calendar-end").into(),
        color_label: locale.tr("calendar-color").into(),
        tip_prev: locale.tr("calendar-tip-prev").into(),
        tip_next: locale.tr("calendar-tip-next").into(),
        tip_prev_year: locale.tr("calendar-tip-prev-year").into(),
        tip_next_year: locale.tr("calendar-tip-next-year").into(),
        tip_today: locale.tr("calendar-tip-today").into(),
        tip_add: locale.tr("calendar-tip-add").into(),
        tip_jump: locale.tr("calendar-tip-jump").into(),
        jump_go_label: locale.tr("calendar-jump-go").into(),
        tip_date_prev: locale.tr("calendar-tip-date-prev").into(),
        tip_date_next: locale.tr("calendar-tip-date-next").into(),
        tip_time_minus: locale
            .tr_args(
                "calendar-tip-time-minus",
                &orchid_i18n::FluentArgs::new().with("minutes", p.time_step_minutes.to_string()),
            )
            .into(),
        tip_time_plus: locale
            .tr_args(
                "calendar-tip-time-plus",
                &orchid_i18n::FluentArgs::new().with("minutes", p.time_step_minutes.to_string()),
            )
            .into(),
        tip_hour_minus: locale.tr("calendar-tip-hour-minus").into(),
        tip_hour_plus: locale.tr("calendar-tip-hour-plus").into(),
    }
}

fn format_hm(hour: i32, minute: i32) -> String {
    let mins = (hour.clamp(0, 23) * 60 + minute.clamp(0, 59)) as u16;
    format_minutes(mins)
}

fn weekday_labels(locale: &LocaleManager, first_day: i32) -> Vec<CalendarWeekdayEntry> {
    let keys = [
        "calendar-weekday-sun",
        "calendar-weekday-mon",
        "calendar-weekday-tue",
        "calendar-weekday-wed",
        "calendar-weekday-thu",
        "calendar-weekday-fri",
        "calendar-weekday-sat",
    ];
    let start = if first_day == 0 { 0 } else { 1 };
    (0..7)
        .map(|i| {
            let idx = (start + i) % 7;
            CalendarWeekdayEntry {
                label: locale.tr(keys[idx]).into(),
            }
        })
        .collect()
}

fn month_name(locale: &LocaleManager, month: i32) -> String {
    let key = match month {
        1 => "calendar-month-jan",
        2 => "calendar-month-feb",
        3 => "calendar-month-mar",
        4 => "calendar-month-apr",
        5 => "calendar-month-may",
        6 => "calendar-month-jun",
        7 => "calendar-month-jul",
        8 => "calendar-month-aug",
        9 => "calendar-month-sep",
        10 => "calendar-month-oct",
        11 => "calendar-month-nov",
        12 => "calendar-month-dec",
        _ => "calendar-month-jan",
    };
    locale.tr(key)
}

fn selected_day_label(locale: &LocaleManager, date_key: &str) -> String {
    let Some(d) = parse_date(date_key) else {
        return date_key.to_string();
    };
    use chrono::Datelike;
    locale.tr_args(
        "calendar-selected-day",
        &orchid_i18n::FluentArgs::new()
            .with(
                "weekday",
                weekday_long(locale, d.weekday().num_days_from_sunday()),
            )
            .with("month", month_name(locale, d.month() as i32))
            .with("day", d.day().to_string()),
    )
}

fn weekday_long(locale: &LocaleManager, from_sunday: u32) -> String {
    let key = match from_sunday {
        0 => "calendar-weekday-long-sun",
        1 => "calendar-weekday-long-mon",
        2 => "calendar-weekday-long-tue",
        3 => "calendar-weekday-long-wed",
        4 => "calendar-weekday-long-thu",
        5 => "calendar-weekday-long-fri",
        _ => "calendar-weekday-long-sat",
    };
    locale.tr(key)
}
