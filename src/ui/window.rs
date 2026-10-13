use crate::ui::dashboard::DashboardView;
use crate::ui::routing::RoutingView;
use crate::ui::servers::ServersView;
use crate::ui::settings::SettingsView;
use crate::ui::subscriptions::SubscriptionsView;
use gtk4::prelude::*;
use gtk4::{Align, Box, Button, Image, Label, ListBox, ListBoxRow, Orientation};
use libadwaita::prelude::*;
use libadwaita::{
    Application, ApplicationWindow, Banner, Breakpoint, BreakpointCondition, HeaderBar,
    OverlaySplitView, ViewStack, ViewSwitcherBar,
};
use std::rc::Rc;

pub struct MainWindow {
    pub window: ApplicationWindow,
    pub dashboard_view: Rc<DashboardView>,
    pub servers_view: Rc<ServersView>,
    pub subscriptions_view: Rc<SubscriptionsView>,
    pub routing_view: Rc<RoutingView>,
    pub settings_view: Rc<SettingsView>,
    pub view_stack: ViewStack,
    pub view_switcher_bar: ViewSwitcherBar,
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
            .icon_name("xdelta-app")
            .default_width(940)
            .default_height(660)
            .build();

        // Crucial for true responsiveness: allow window to shrink down to 360px without being blocked
        window.set_size_request(360, 480);

        // Adaptive OverlaySplitView (GNOME 45+ HIG)
        let split_view = OverlaySplitView::new();
        split_view.set_min_sidebar_width(220.0);
        split_view.set_max_sidebar_width(260.0);
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

        let logo_icon = Image::from_icon_name("xdelta");
        logo_icon.set_pixel_size(26);
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
            .visible(false) // Initially hidden on wide desktop
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

        // Libadwaita ViewStack
        let view_stack = ViewStack::new();
        view_stack.set_vexpand(true);
        view_stack.set_hexpand(true);

        let dashboard_view = Rc::new(DashboardView::new());
        let servers_view = Rc::new(ServersView::new());
        let subscriptions_view = Rc::new(SubscriptionsView::new());
        let routing_view = Rc::new(RoutingView::new());
        let settings_view = Rc::new(SettingsView::new());

        let dash_page =
            view_stack.add_titled(&dashboard_view.container, Some("dashboard"), "Dashboard");
        dash_page.set_icon_name(Some("speedometer-symbolic"));

        let srv_page = view_stack.add_titled(&servers_view.container, Some("servers"), "Servers");
        srv_page.set_icon_name(Some("network-server-symbolic"));

        let subs_page = view_stack.add_titled(
            &subscriptions_view.container,
            Some("subscriptions"),
            "Subscriptions",
        );
        subs_page.set_icon_name(Some("folder-download-symbolic"));

        let route_page = view_stack.add_titled(&routing_view.container, Some("routing"), "Routing");
        route_page.set_icon_name(Some("route-symbolic"));

        let sett_page =
            view_stack.add_titled(&settings_view.container, Some("settings"), "Settings");
        sett_page.set_icon_name(Some("preferences-system-symbolic"));

        // Bottom Navigation Bar for Compact & Mobile View
        let view_switcher_bar = ViewSwitcherBar::builder()
            .stack(&view_stack)
            .reveal(false) // Hidden on desktop, revealed on compact/tiled
            .build();

        right_container.append(&header_bar);
        right_container.append(&update_banner);
        right_container.append(&view_stack);
        right_container.append(&view_switcher_bar);

        // Synchronize ViewStack page changes to Header Title
        let title_stack_clone = page_title_label.clone();
        view_stack.connect_visible_child_name_notify(move |st| {
            if let Some(name) = st.visible_child_name() {
                match name.as_str() {
                    "dashboard" => title_stack_clone.set_label("Dashboard"),
                    "servers" => title_stack_clone.set_label("Servers"),
                    "subscriptions" => title_stack_clone.set_label("Subscriptions"),
                    "routing" => title_stack_clone.set_label("Routing Rules"),
                    "settings" => title_stack_clone.set_label("Settings"),
                    _ => {}
                }
            }
        });

        // Wire Sidebar Navigation to ViewStack
        let stack_clone = view_stack.clone();
        let split_collapse_clone = split_view.clone();

        nav_list.connect_row_selected(move |_, row| {
            if let Some(r) = row {
                match r.index() {
                    0 => stack_clone.set_visible_child_name("dashboard"),
                    1 => stack_clone.set_visible_child_name("servers"),
                    2 => stack_clone.set_visible_child_name("subscriptions"),
                    3 => stack_clone.set_visible_child_name("routing"),
                    4 => stack_clone.set_visible_child_name("settings"),
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

        // Dynamically toggle bottom bar and toggle button when collapsed state changes
        let switcher_collapse_clone = view_switcher_bar.clone();
        let toggle_btn_collapse_clone = sidebar_toggle_btn.clone();
        split_view.connect_collapsed_notify(move |split| {
            let is_col = split.is_collapsed();
            switcher_collapse_clone.set_reveal(is_col);
            toggle_btn_collapse_clone.set_visible(is_col);
        });

        // 3. Responsive Breakpoint (Libadwaita Breakpoint at 740px)
        if let Ok(cond) = BreakpointCondition::parse("max-width: 740sp") {
            let breakpoint = Breakpoint::new(cond);
            breakpoint.add_setter(&split_view, "collapsed", Some(&true.to_value()));
            breakpoint.add_setter(&sidebar_toggle_btn, "visible", Some(&true.to_value()));
            breakpoint.add_setter(&view_switcher_bar, "reveal", Some(&true.to_value()));
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
            view_stack,
            view_switcher_bar,
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
