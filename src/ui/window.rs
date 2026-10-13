use crate::ui::dashboard::DashboardView;
use crate::ui::routing::RoutingView;
use crate::ui::servers::ServersView;
use crate::ui::settings::SettingsView;
use crate::ui::subscriptions::SubscriptionsView;
use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, Image, Label, ListBox, ListBoxRow, Orientation, Stack, StackTransitionType,
};
use libadwaita::prelude::*;
use libadwaita::{
    Application, ApplicationWindow, Banner, Breakpoint, BreakpointCondition, HeaderBar,
    OverlaySplitView,
};
use std::rc::Rc;

pub struct MainWindow {
    pub window: ApplicationWindow,
    pub dashboard_view: Rc<DashboardView>,
    pub servers_view: Rc<ServersView>,
    pub subscriptions_view: Rc<SubscriptionsView>,
    pub routing_view: Rc<RoutingView>,
    pub settings_view: Rc<SettingsView>,
    pub content_stack: Stack,
    pub nav_list: ListBox,
    pub page_title_label: Label,
    pub split_view: OverlaySplitView,
    pub update_banner: Banner,
}

impl MainWindow {
    pub fn new(app: &Application) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("xdelta")
            .default_width(980)
            .default_height(690)
            .build();

        // Responsive Adaptive OverlaySplitView (GNOME 45+ HIG)
        let split_view = OverlaySplitView::new();
        split_view.set_min_sidebar_width(220.0);
        split_view.set_max_sidebar_width(280.0);
        split_view.set_sidebar_width_fraction(0.24);
        split_view.set_enable_show_gesture(true);
        split_view.set_enable_hide_gesture(true);

        // ==========================================
        // 1. ADAPTIVE SIDEBAR
        // ==========================================
        let sidebar = Box::builder().orientation(Orientation::Vertical).build();
        sidebar.add_css_class("sidebar-pane");

        // Brand & Title
        let brand_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .valign(Align::Center)
            .build();
        brand_box.add_css_class("sidebar-brand-box");

        let logo_icon = Image::from_icon_name("network-vpn-symbolic");
        logo_icon.add_css_class("sidebar-logo");

        let title_label = Label::builder()
            .label("xdelta")
            .halign(Align::Start)
            .build();
        title_label.add_css_class("sidebar-brand-title");

        let ver_badge = Label::builder().label("v1.0").build();
        ver_badge.add_css_class("sidebar-version-badge");

        brand_box.append(&logo_icon);
        brand_box.append(&title_label);
        brand_box.append(&ver_badge);

        // Navigation List
        let nav_list = ListBox::builder()
            .selection_mode(gtk4::SelectionMode::Single)
            .vexpand(true)
            .build();
        nav_list.add_css_class("sidebar-nav-list");

        let dash_row = Self::create_nav_row("speedometer-symbolic", "Dashboard");
        let servers_row = Self::create_nav_row("network-server-symbolic", "Servers");
        let subs_row = Self::create_nav_row("folder-download-symbolic", "Subscriptions");
        let routing_row = Self::create_nav_row("route-symbolic", "Routing");
        let settings_row = Self::create_nav_row("preferences-system-symbolic", "Settings");

        nav_list.append(&dash_row);
        nav_list.append(&servers_row);
        nav_list.append(&subs_row);
        nav_list.append(&routing_row);
        nav_list.append(&settings_row);

        // Sidebar Footer (Active Core Indicator)
        let footer_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .halign(Align::Fill)
            .build();
        footer_box.add_css_class("sidebar-footer");

        let core_pill = Label::builder()
            .label("● Xray Core (Active)")
            .hexpand(true)
            .halign(Align::Start)
            .build();
        core_pill.add_css_class("core-badge");

        footer_box.append(&core_pill);

        sidebar.append(&brand_box);
        sidebar.append(&nav_list);
        sidebar.append(&footer_box);

        // ==========================================
        // 2. RIGHT CONTENT AREA
        // ==========================================
        let right_container = Box::builder()
            .orientation(Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();

        // Top HeaderBar (Minimal & Native Libadwaita)
        let header_bar = HeaderBar::new();
        let page_title_label = Label::builder().label("Dashboard").build();
        page_title_label.add_css_class("title-3");
        header_bar.set_title_widget(Some(&page_title_label));

        // Responsive Sidebar Toggle Button (active in narrow / collapsed mode)
        let sidebar_toggle_btn = Button::builder()
            .icon_name("view-sidebar-symbolic")
            .tooltip_text("Toggle Navigation")
            .build();
        sidebar_toggle_btn.add_css_class("flat");

        let split_toggle_clone = split_view.clone();
        sidebar_toggle_btn.connect_clicked(move |_| {
            split_toggle_clone.set_show_sidebar(!split_toggle_clone.shows_sidebar());
        });
        header_bar.pack_start(&sidebar_toggle_btn);

        // In-app Update Banner (revealed dynamically if new release is found)
        let update_banner = Banner::builder()
            .title("⚡ A new version of xdelta is available on GitHub")
            .button_label("View Release")
            .use_markup(true)
            .build();
        update_banner.set_revealed(false);

        update_banner.connect_button_clicked(|_| {
            let _ = gio::AppInfo::launch_default_for_uri(
                "https://github.com/TssHack/xdelta/releases",
                None::<&gio::AppLaunchContext>,
            );
        });

        // Content Stack
        let content_stack = Stack::builder()
            .transition_type(StackTransitionType::Crossfade)
            .transition_duration(180)
            .vexpand(true)
            .hexpand(true)
            .build();

        let dashboard_view = Rc::new(DashboardView::new());
        let servers_view = Rc::new(ServersView::new());
        let subscriptions_view = Rc::new(SubscriptionsView::new());
        let routing_view = Rc::new(RoutingView::new());
        let settings_view = Rc::new(SettingsView::new());

        content_stack.add_named(&dashboard_view.container, Some("dashboard"));
        content_stack.add_named(&servers_view.container, Some("servers"));
        content_stack.add_named(&subscriptions_view.container, Some("subscriptions"));
        content_stack.add_named(&routing_view.container, Some("routing"));
        content_stack.add_named(&settings_view.container, Some("settings"));

        right_container.append(&header_bar);
        right_container.append(&update_banner);
        right_container.append(&content_stack);

        // Wire Sidebar Navigation to Stack Pages
        let stack_clone = content_stack.clone();
        let title_clone = page_title_label.clone();
        let split_collapse_clone = split_view.clone();

        nav_list.connect_row_selected(move |_, row| {
            if let Some(r) = row {
                match r.index() {
                    0 => {
                        stack_clone.set_visible_child_name("dashboard");
                        title_clone.set_label("Dashboard");
                    }
                    1 => {
                        stack_clone.set_visible_child_name("servers");
                        title_clone.set_label("Servers");
                    }
                    2 => {
                        stack_clone.set_visible_child_name("subscriptions");
                        title_clone.set_label("Subscriptions");
                    }
                    3 => {
                        stack_clone.set_visible_child_name("routing");
                        title_clone.set_label("Routing Rules");
                    }
                    4 => {
                        stack_clone.set_visible_child_name("settings");
                        title_clone.set_label("Settings");
                    }
                    _ => {}
                }

                // Auto-close overlay sidebar on mobile/narrow screen upon selection
                if split_collapse_clone.is_collapsed() {
                    split_collapse_clone.set_show_sidebar(false);
                }
            }
        });

        // Select Dashboard initially
        nav_list.select_row(Some(&dash_row));

        split_view.set_sidebar(Some(&sidebar));
        split_view.set_content(Some(&right_container));

        // 3. Responsive Breakpoint (Libadwaita Breakpoint at 720px)
        if let Ok(cond) = BreakpointCondition::parse("max-width: 720sp") {
            let breakpoint = Breakpoint::new(cond);
            breakpoint.add_setter(&split_view, "collapsed", Some(&true.to_value()));
            window.add_breakpoint(breakpoint);
        }

        window.set_content(Some(&split_view));

        Self {
            window,
            dashboard_view,
            servers_view,
            subscriptions_view,
            routing_view,
            settings_view,
            content_stack,
            nav_list,
            page_title_label,
            split_view,
            update_banner,
        }
    }

    fn create_nav_row(icon_name: &str, title: &str) -> ListBoxRow {
        let row = ListBoxRow::new();
        row.add_css_class("sidebar-nav-row");

        let row_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .build();

        let icon = Image::from_icon_name(icon_name);
        let label = Label::builder().label(title).build();

        row_box.append(&icon);
        row_box.append(&label);
        row.set_child(Some(&row_box));

        row
    }

    pub fn present(&self) {
        self.window.present();
    }
}
