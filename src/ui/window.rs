use crate::ui::dashboard::DashboardView;
use crate::ui::servers::ServersView;
use crate::ui::subscriptions::SubscriptionsView;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use libadwaita::{
    Application, ApplicationWindow, HeaderBar, ToolbarView, ViewStack, ViewSwitcher,
    ViewSwitcherPolicy,
};
use std::rc::Rc;

pub struct MainWindow {
    pub window: ApplicationWindow,
    pub dashboard_view: Rc<DashboardView>,
    pub servers_view: Rc<ServersView>,
    pub subscriptions_view: Rc<SubscriptionsView>,
    pub view_stack: ViewStack,
}

impl MainWindow {
    pub fn new(app: &Application) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("xdelta")
            .default_width(780)
            .default_height(620)
            .build();

        let toolbar_view = ToolbarView::new();
        let header_bar = HeaderBar::new();

        let view_stack = ViewStack::new();

        // 1. Dashboard View
        let dashboard_view = Rc::new(DashboardView::new());
        let dash_page =
            view_stack.add_titled(&dashboard_view.container, Some("dashboard"), "Dashboard");
        dash_page.set_icon_name(Some("network-workgroup-symbolic"));

        // 2. Servers View
        let servers_view = Rc::new(ServersView::new());
        let servers_page =
            view_stack.add_titled(&servers_view.container, Some("servers"), "Servers");
        servers_page.set_icon_name(Some("network-server-symbolic"));

        // 3. Subscriptions View
        let subscriptions_view = Rc::new(SubscriptionsView::new());
        let subs_page = view_stack.add_titled(
            &subscriptions_view.container,
            Some("subscriptions"),
            "Subscriptions",
        );
        subs_page.set_icon_name(Some("folder-download-symbolic"));

        // Header View Switcher
        let switcher = ViewSwitcher::builder()
            .stack(&view_stack)
            .policy(ViewSwitcherPolicy::Wide)
            .build();

        header_bar.set_title_widget(Some(&switcher));
        toolbar_view.add_top_bar(&header_bar);
        toolbar_view.set_content(Some(&view_stack));

        window.set_content(Some(&toolbar_view));

        Self {
            window,
            dashboard_view,
            servers_view,
            subscriptions_view,
            view_stack,
        }
    }

    pub fn present(&self) {
        self.window.present();
    }
}
