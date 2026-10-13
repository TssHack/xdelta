use crate::ui::dashboard::DashboardView;
use crate::ui::servers::ServersView;
use crate::ui::subscriptions::SubscriptionsView;
use gtk4::prelude::*;
use gtk4::{
    Align, Box, HeaderBar, Image, Label, ListBox, ListBoxRow, Orientation, Stack,
    StackTransitionType,
};
use libadwaita::prelude::*;
use libadwaita::Application;
use libadwaita::ApplicationWindow;
use std::rc::Rc;

pub struct MainWindow {
    pub window: ApplicationWindow,
    pub dashboard_view: Rc<DashboardView>,
    pub servers_view: Rc<ServersView>,
    pub subscriptions_view: Rc<SubscriptionsView>,
    pub content_stack: Stack,
    pub nav_list: ListBox,
    pub page_title_label: Label,
}

impl MainWindow {
    pub fn new(app: &Application) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("xdelta")
            .default_width(960)
            .default_height(680)
            .build();

        // Main Horizontal Split: Left Sidebar + Right Content
        let main_layout = Box::builder().orientation(Orientation::Horizontal).build();

        // ==========================================
        // 1. LEFT SIDEBAR
        // ==========================================
        let sidebar = Box::builder()
            .orientation(Orientation::Vertical)
            .width_request(240)
            .build();
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

        nav_list.append(&dash_row);
        nav_list.append(&servers_row);
        nav_list.append(&subs_row);

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

        // Top HeaderBar (Minimal & Native)
        let header_bar = HeaderBar::new();
        let page_title_label = Label::builder().label("Dashboard").build();
        page_title_label.add_css_class("title-3");
        header_bar.set_title_widget(Some(&page_title_label));

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

        content_stack.add_named(&dashboard_view.container, Some("dashboard"));
        content_stack.add_named(&servers_view.container, Some("servers"));
        content_stack.add_named(&subscriptions_view.container, Some("subscriptions"));

        right_container.append(&header_bar);
        right_container.append(&content_stack);

        // Wire Sidebar Navigation to Stack Pages
        let stack_clone = content_stack.clone();
        let title_clone = page_title_label.clone();
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
                    _ => {}
                }
            }
        });

        // Select Dashboard initially
        nav_list.select_row(Some(&dash_row));

        main_layout.append(&sidebar);
        main_layout.append(&right_container);

        window.set_content(Some(&main_layout));

        Self {
            window,
            dashboard_view,
            servers_view,
            subscriptions_view,
            content_stack,
            nav_list,
            page_title_label,
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
