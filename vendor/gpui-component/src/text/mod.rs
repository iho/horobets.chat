mod format;
mod inline;
mod node;
mod style;
mod text_view;
mod utils;

use gpui::App;
pub use style::*;
pub use text_view::*;

pub(crate) fn init(cx: &mut App) {
    text_view::init(cx);
}


/// Allows applications to route internal text links before falling back to the OS browser.
#[derive(Clone)]
pub struct LinkHandler(pub std::rc::Rc<dyn Fn(&str, &mut gpui::Window, &mut App) -> bool>);
impl gpui::Global for LinkHandler {}

pub(crate) fn open_link(url: &str, window: &mut gpui::Window, cx: &mut App) {
    let handler = cx.try_global::<LinkHandler>().cloned();
    if handler.is_some_and(|handler| (handler.0)(url, window, cx)) {
        return;
    }
    cx.open_url(url);
}
