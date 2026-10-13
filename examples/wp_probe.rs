//! Throwaway probe: does building the wallpaper gallery spuriously fire
//! `toggled` handlers (and therefore overwrite the user's saved choice)?
//!
//! Run: DISPLAY=:0 cargo run --example wp_probe

use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

const IDS: &[&str] = &["drift", "bloom", "evelyn", "profile", "velocity"];
const SAVED: &str = "drift";

fn main() {
    gtk4::init().expect("GTK init");

    let events: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let mut group_leader: Option<gtk4::ToggleButton> = None;
    let buttons: Rc<RefCell<Vec<(String, gtk4::ToggleButton)>>> =
        Rc::new(RefCell::new(Vec::new()));

    for id in IDS {
        let button = gtk4::ToggleButton::builder().label(*id).build();

        match &group_leader {
            Some(leader) => button.set_group(Some(leader)),
            None => group_leader = Some(button.clone()),
        }

        if *id == SAVED {
            button.set_active(true);
        }

        let events_clone = events.clone();
        let id_owned = id.to_string();
        button.connect_toggled(move |btn| {
            if !btn.is_active() {
                return;
            }
            events_clone.borrow_mut().push(id_owned.clone());
        });

        buttons.borrow_mut().push((id.to_string(), button));
    }

    let wins: Vec<String> = buttons
        .borrow()
        .iter()
        .filter(|(_, b)| b.is_active())
        .map(|(id, _)| id.clone())
        .collect();

    println!("saved id            : {}", SAVED);
    println!("active after setup  : {:?}", wins);
    println!("spurious toggles    : {:?}", events.borrow());

    if wins == vec![SAVED.to_string()] && events.borrow().is_empty() {
        println!("RESULT: clean setup, no stray writes");
    } else {
        println!("RESULT: BUG - setup is not clean");
    }
}
