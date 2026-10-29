//! Exercise the real AppWindow, including focus transfer and the RPC callback.
use crate::{cover_flow, AlbumItem, AppWindow};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Key, Platform, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

struct Headless(Rc<MinimalSoftwareWindow>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn key(app: &AppWindow, text: slint::SharedString) {
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

#[test]
fn cover_flow_shortcuts_search_navigation_and_album_details() {
    slint::platform::set_platform(Box::new(Headless(MinimalSoftwareWindow::new(
        RepaintBufferType::NewBuffer,
    ))))
    .unwrap();
    let app = AppWindow::new().unwrap();
    app.on_filter_cover_albums(cover_flow::filter_albums);
    let cache = RefCell::new(cover_flow::ProjectionCache::default());
    app.on_project_cover(move |art, turn| cache.borrow_mut().project(art, turn));
    let opened = Rc::new(RefCell::new(String::new()));
    let request = opened.clone();
    app.on_open_album(move |id| *request.borrow_mut() = id.to_string());
    app.set_albums(ModelRc::new(VecModel::from(vec![
        AlbumItem {
            id: "debut".into(),
            title: "Début".into(),
            artist: "Björk".into(),
            ..Default::default()
        },
        AlbumItem {
            id: "discovery".into(),
            title: "Discovery".into(),
            artist: "Daft Punk".into(),
            ..Default::default()
        },
        AlbumItem {
            id: "ram".into(),
            title: "Random Access Memories".into(),
            artist: "Daft Punk".into(),
            ..Default::default()
        },
    ])));
    app.window().set_size(slint::LogicalSize::new(1200., 780.));
    app.show().unwrap();

    app.set_show_full_player(true);
    key(&app, "c".into());
    assert!(app.get_show_cover_flow());
    key(&app, Key::Escape.into());
    assert!(!app.get_show_cover_flow());
    assert!(
        app.get_show_full_player(),
        "Escape restores the previous view"
    );
    key(&app, "c".into());
    key(&app, Key::RightArrow.into());
    key(&app, Key::Return.into());
    assert!(!app.get_show_cover_flow());
    assert!(!app.get_show_full_player());
    assert!(app.get_show_detail());
    assert_eq!(app.get_detail_album().id, "discovery");
    assert_eq!(
        *opened.borrow(),
        "discovery",
        "album details must reach the RPC callback"
    );

    key(&app, "c".into());
    key(&app, "/".into());
    for ch in "PUNK".chars() {
        key(&app, ch.to_string().into());
    }
    assert!(
        !app.get_show_palette(),
        "Cover Flow search must not open the global palette"
    );
    key(&app, Key::DownArrow.into());
    key(&app, Key::Return.into());
    assert!(!app.get_show_cover_flow());
    assert_eq!(app.get_detail_album().id, "ram");
    assert_eq!(*opened.borrow(), "ram");

    key(&app, "c".into());
    key(&app, "/".into());
    for ch in "czzz".chars() {
        key(&app, ch.to_string().into());
    }
    assert!(
        app.get_show_cover_flow(),
        "c must type into search rather than toggle the view"
    );
    key(&app, Key::Return.into());
    assert!(
        app.get_show_cover_flow(),
        "empty search results cannot open an album"
    );
    assert_eq!(*opened.borrow(), "ram");
    key(&app, Key::Escape.into());
    assert!(app.get_show_cover_flow(), "first Escape clears search");
    key(&app, Key::Escape.into());
    assert!(!app.get_show_cover_flow(), "second Escape exits");
    key(&app, "c".into());
    key(&app, "c".into());
    assert!(
        !app.get_show_cover_flow(),
        "reopening must restore keyboard focus"
    );

    key(&app, "c".into());
    key(&app, Key::Home.into());
    for _ in 0..7 {
        app.window().dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(600., 300.),
            delta_x: -10.,
            delta_y: 2.,
        });
    }
    key(&app, Key::Return.into());
    assert_eq!(
        app.get_detail_album().id,
        "debut",
        "tiny trackpad events must accumulate"
    );
    key(&app, "c".into());
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(600., 300.),
        delta_x: -80.,
        delta_y: 2.,
    });
    key(&app, Key::Return.into());
    assert_eq!(
        app.get_detail_album().id,
        "discovery",
        "scrolling over a cover advances one album"
    );

    key(&app, "c".into());
    key(&app, Key::Home.into());
    for event in [
        WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(600., 300.),
            button: slint::platform::PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(600., 300.),
            button: slint::platform::PointerEventButton::Left,
        },
    ] {
        app.window().dispatch_event(event);
    }
    assert!(
        !app.get_show_cover_flow(),
        "click opens the visible cover's details"
    );
    assert!(app.get_show_detail());

    app.set_albums(ModelRc::new(VecModel::from(Vec::<AlbumItem>::new())));
    key(&app, "c".into());
    key(&app, Key::RightArrow.into());
    key(&app, Key::Return.into());
    assert!(app.get_show_cover_flow());
    key(&app, Key::Escape.into());
    key(&app, "/".into());
    assert!(
        app.get_show_palette(),
        "global shortcuts work after Cover Flow closes"
    );
}
