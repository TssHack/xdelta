use gtk4::gdk::Display;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, DropDown, Label, Orientation, ScrolledWindow, SearchEntry, TextBuffer,
    TextTag, TextView, ToggleButton,
};
use std::cell::RefCell;
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogFilterLevel {
    All,
    Info,
    Warn,
    Error,
    Core,
}

impl LogFilterLevel {
    pub fn from_index(index: u32) -> Self {
        match index {
            1 => Self::Info,
            2 => Self::Warn,
            3 => Self::Error,
            4 => Self::Core,
            _ => Self::All,
        }
    }

    pub fn matches(&self, line: &str) -> bool {
        match self {
            Self::All => true,
            Self::Info => line.contains("INFO"),
            Self::Warn => line.contains("WARN"),
            Self::Error => line.contains("ERROR"),
            Self::Core => {
                line.contains("[core]") || line.contains("xray") || line.contains("sing-box")
            }
        }
    }
}

pub struct LogsView {
    pub container: Box,
    pub text_view: TextView,
    pub buffer: TextBuffer,
    pub auto_scroll: Rc<RefCell<bool>>,
    pub search_query: Rc<RefCell<String>>,
    pub filter_level: Rc<RefCell<LogFilterLevel>>,
    pub all_lines: Rc<RefCell<Vec<String>>>,
    pub file_offset: Rc<RefCell<u64>>,
}

impl Default for LogsView {
    fn default() -> Self {
        Self::new()
    }
}

impl LogsView {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .hexpand(true)
            .vexpand(true)
            .build();
        container.add_css_class("logs-page-container");

        // Top Toolbar
        let toolbar = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .margin_start(16)
            .margin_end(16)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        toolbar.add_css_class("logs-toolbar");

        // Search entry
        let search_entry = SearchEntry::builder()
            .placeholder_text("Filter logs...")
            .hexpand(true)
            .build();

        // Level DropDown
        let levels =
            gtk4::StringList::new(&["All Levels", "Info", "Warnings", "Errors", "Core Output"]);
        let level_dropdown = DropDown::builder()
            .model(&levels)
            .selected(0)
            .tooltip_text("Filter by log severity")
            .build();

        // Auto-scroll Toggle
        let auto_scroll_toggle = ToggleButton::builder()
            .icon_name("go-bottom-symbolic")
            .active(true)
            .tooltip_text("Auto-scroll to latest log entries")
            .build();
        auto_scroll_toggle.add_css_class("flat");

        // Copy button
        let copy_btn = Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text("Copy logs to clipboard")
            .build();
        copy_btn.add_css_class("flat");

        // Clear display button
        let clear_btn = Button::builder()
            .icon_name("edit-clear-symbolic")
            .tooltip_text("Clear current log buffer")
            .build();
        clear_btn.add_css_class("flat");

        toolbar.append(&search_entry);
        toolbar.append(&level_dropdown);
        toolbar.append(&auto_scroll_toggle);
        toolbar.append(&copy_btn);
        toolbar.append(&clear_btn);

        // TextBuffer and TextView
        let buffer = TextBuffer::new(None);

        // Setup syntax highlighting tags
        let tag_time = TextTag::builder()
            .name("timestamp")
            .foreground("rgba(160, 175, 200, 0.65)")
            .build();
        let tag_info = TextTag::builder()
            .name("info")
            .foreground("#3584e4")
            .weight(700)
            .build();
        let tag_warn = TextTag::builder()
            .name("warn")
            .foreground("#f5c211")
            .weight(700)
            .build();
        let tag_error = TextTag::builder()
            .name("error")
            .foreground("#e01b24")
            .weight(700)
            .build();
        let tag_core = TextTag::builder()
            .name("core")
            .foreground("#2ec27e")
            .weight(700)
            .build();

        let tag_table = buffer.tag_table();
        tag_table.add(&tag_time);
        tag_table.add(&tag_info);
        tag_table.add(&tag_warn);
        tag_table.add(&tag_error);
        tag_table.add(&tag_core);

        let text_view = TextView::builder()
            .buffer(&buffer)
            .editable(false)
            .cursor_visible(false)
            .monospace(true)
            .wrap_mode(gtk4::WrapMode::WordChar)
            .top_margin(12)
            .bottom_margin(12)
            .left_margin(16)
            .right_margin(16)
            .build();
        text_view.add_css_class("logs-console");

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&text_view)
            .vexpand(true)
            .hexpand(true)
            .build();
        scrolled.add_css_class("logs-scrolled-window");

        // Bottom status footer
        let footer = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .margin_start(16)
            .margin_end(16)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        footer.add_css_class("logs-footer");

        let log_file_label = Label::builder()
            .label("File: ~/.config/xdelta/xdelta.log")
            .halign(Align::Start)
            .hexpand(true)
            .build();
        log_file_label.add_css_class("caption");
        log_file_label.add_css_class("dim-label");

        let line_count_label = Label::builder().label("0 lines").halign(Align::End).build();
        line_count_label.add_css_class("caption");
        line_count_label.add_css_class("dim-label");

        footer.append(&log_file_label);
        footer.append(&line_count_label);

        container.append(&toolbar);
        container.append(&scrolled);
        container.append(&footer);

        let auto_scroll = Rc::new(RefCell::new(true));
        let search_query = Rc::new(RefCell::new(String::new()));
        let filter_level = Rc::new(RefCell::new(LogFilterLevel::All));
        let all_lines = Rc::new(RefCell::new(Vec::<String>::new()));
        let file_offset = Rc::new(RefCell::new(0u64));

        // Auto-scroll toggle wire
        let as_clone = auto_scroll.clone();
        auto_scroll_toggle.connect_toggled(move |btn| {
            *as_clone.borrow_mut() = btn.is_active();
        });

        // Copy button wire
        let buf_copy = buffer.clone();
        copy_btn.connect_clicked(move |_| {
            let (start, end) = buf_copy.bounds();
            let text = buf_copy.text(&start, &end, false);
            if let Some(display) = Display::default() {
                display.clipboard().set_text(&text);
            }
        });

        // Clear button wire
        let buf_clear = buffer.clone();
        let lines_clear = all_lines.clone();
        let counter_clear = line_count_label.clone();
        clear_btn.connect_clicked(move |_| {
            buf_clear.set_text("");
            lines_clear.borrow_mut().clear();
            counter_clear.set_label("0 lines");
        });

        // Search and filter wires
        let query_clone = search_query.clone();
        let fl_clone = filter_level.clone();
        let all_clone = all_lines.clone();
        let buf_reapply = buffer.clone();
        let counter_reapply = line_count_label.clone();
        let tv_reapply = text_view.clone();
        let as_reapply = auto_scroll.clone();

        let reapply_filters = Rc::new(move || {
            let q = query_clone.borrow().to_lowercase();
            let level = *fl_clone.borrow();
            let lines = all_clone.borrow();

            buf_reapply.set_text("");
            let mut matched_count = 0;

            for line in lines.iter() {
                if !level.matches(line) {
                    continue;
                }
                if !q.is_empty() && !line.to_lowercase().contains(&q) {
                    continue;
                }

                Self::append_styled_line(&buf_reapply, line);
                matched_count += 1;
            }

            counter_reapply.set_label(&format!("{} lines", matched_count));

            if *as_reapply.borrow() {
                Self::scroll_to_bottom(&tv_reapply, &buf_reapply);
            }
        });

        let reapply_search = reapply_filters.clone();
        let q_store = search_query.clone();
        search_entry.connect_search_changed(move |entry| {
            *q_store.borrow_mut() = entry.text().to_string();
            reapply_search();
        });

        let reapply_lvl = reapply_filters.clone();
        let fl_store = filter_level.clone();
        level_dropdown.connect_selected_notify(move |dd| {
            *fl_store.borrow_mut() = LogFilterLevel::from_index(dd.selected());
            reapply_lvl();
        });

        // Periodic incremental log reader
        let log_path = crate::core::persist::log_path();
        let lines_periodic = all_lines.clone();
        let offset_periodic = file_offset.clone();
        let buf_periodic = buffer.clone();
        let tv_periodic = text_view.clone();
        let as_periodic = auto_scroll.clone();
        let fl_periodic = filter_level.clone();
        let q_periodic = search_query.clone();
        let counter_periodic = line_count_label;

        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let Some(ref path) = log_path else {
                return glib::ControlFlow::Continue;
            };

            let Ok(mut file) = File::open(path) else {
                return glib::ControlFlow::Continue;
            };

            let current_len = file.metadata().map(|m| m.len()).unwrap_or(0);
            let mut offset = *offset_periodic.borrow();

            // If file was truncated or rotated, reset
            if current_len < offset {
                offset = 0;
            }

            if current_len > offset && file.seek(SeekFrom::Start(offset)).is_ok() {
                let mut reader = std::io::BufReader::new(file);
                use std::io::BufRead;
                let mut new_lines = Vec::new();
                let mut bytes_read = 0;

                let mut line_buf = String::new();
                while let Ok(n) = reader.read_line(&mut line_buf) {
                    if n == 0 {
                        break;
                    }
                    bytes_read += n as u64;
                    let trimmed = line_buf.trim_end_matches(&['\r', '\n'][..]).to_string();
                    if !trimmed.is_empty() {
                        new_lines.push(trimmed);
                    }
                    line_buf.clear();
                }

                *offset_periodic.borrow_mut() = offset + bytes_read;

                if !new_lines.is_empty() {
                    let level = *fl_periodic.borrow();
                    let q = q_periodic.borrow().to_lowercase();
                    let mut added_to_view = false;

                    let mut all = lines_periodic.borrow_mut();
                    for line in new_lines {
                        all.push(line.clone());

                        if level.matches(&line)
                            && (q.is_empty() || line.to_lowercase().contains(&q))
                        {
                            Self::append_styled_line(&buf_periodic, &line);
                            added_to_view = true;
                        }
                    }

                    counter_periodic.set_label(&format!("{} lines", all.len()));

                    if added_to_view && *as_periodic.borrow() {
                        Self::scroll_to_bottom(&tv_periodic, &buf_periodic);
                    }
                }
            }

            glib::ControlFlow::Continue
        });

        Self {
            container,
            text_view,
            buffer,
            auto_scroll,
            search_query,
            filter_level,
            all_lines,
            file_offset,
        }
    }

    fn append_styled_line(buffer: &TextBuffer, line: &str) {
        let mut iter = buffer.end_iter();

        // Check if line contains timestamp e.g. [2026-10-13T17:57:37.836Z
        if line.starts_with('[') && line.contains(']') {
            if let Some(close_idx) = line.find(']') {
                let (prefix, rest) = line.split_at(close_idx + 1);
                buffer.insert_with_tags_by_name(&mut iter, prefix, &["timestamp"]);

                // Highlight severity level in the rest of the line
                if rest.contains("INFO") {
                    buffer.insert_with_tags_by_name(&mut iter, rest, &["info"]);
                } else if rest.contains("WARN") {
                    buffer.insert_with_tags_by_name(&mut iter, rest, &["warn"]);
                } else if rest.contains("ERROR") {
                    buffer.insert_with_tags_by_name(&mut iter, rest, &["error"]);
                } else if rest.contains("[core]") || rest.contains("xray") {
                    buffer.insert_with_tags_by_name(&mut iter, rest, &["core"]);
                } else {
                    buffer.insert(&mut iter, rest);
                }
                buffer.insert(&mut iter, "\n");
                return;
            }
        }

        // Fallback coloring by keyword
        if line.contains("ERROR") || line.contains("failed") || line.contains("Failed") {
            buffer.insert_with_tags_by_name(&mut iter, line, &["error"]);
        } else if line.contains("WARN") || line.contains("warning") {
            buffer.insert_with_tags_by_name(&mut iter, line, &["warn"]);
        } else if line.contains("[core]") || line.contains("core reported") {
            buffer.insert_with_tags_by_name(&mut iter, line, &["core"]);
        } else {
            buffer.insert(&mut iter, line);
        }
        buffer.insert(&mut iter, "\n");
    }

    fn scroll_to_bottom(text_view: &TextView, buffer: &TextBuffer) {
        let end_iter = buffer.end_iter();
        let mark = buffer.create_mark(None, &end_iter, false);
        text_view.scroll_to_mark(&mark, 0.0, true, 0.0, 1.0);
    }
}
