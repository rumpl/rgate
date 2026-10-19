use gpui::{App, AppContext, Entity, SharedString, Window};
use gpui_component::input::{InputState, TextareaState};

pub fn text_field(
    value: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    let value = value.into();
    cx.new(|cx| InputState::new(window, cx).default_value(value))
}

pub fn search_field(
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

pub fn text_area(
    value: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<TextareaState> {
    let value = value.into();
    cx.new(|cx| TextareaState::new(window, cx).default_value(value))
}
