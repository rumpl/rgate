use crate::{
    app::{Dialog, GateApp},
    canvas::{stroke, text},
    vector::Shape,
};
use gpui::{
    AnyElement, App, Bounds, Context, MouseButton, Pixels, Window, canvas, div, point, prelude::*,
    px, rgb,
};
use rgate_core::{Point, SymbolPrimitive};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SymbolTool {
    Line,
    Rectangle,
    Ellipse,
    Text,
}
pub struct SymbolDialog {
    pub module: String,
    pub shapes: Vec<SymbolPrimitive>,
    pub tool: SymbolTool,
    pub start: Option<Point>,
    pub bounds: Bounds<Pixels>,
    pub text: gpui::Entity<crate::input::TextField>,
}
impl SymbolDialog {
    pub fn local(&self, point: gpui::Point<Pixels>) -> Point {
        Point::new(
            (f32::from(point.x - self.bounds.center().x)) / 2.0,
            (f32::from(point.y - self.bounds.center().y)) / 2.0,
        )
        .snapped(5.0)
    }
}

pub fn editor(app: &GateApp, cx: &mut Context<GateApp>) -> AnyElement {
    let Some(Dialog::Symbol(dialog)) = &app.dialog else {
        return div().into_any_element();
    };
    let shapes = dialog.shapes.clone();
    let palette = app.theme.palette();
    let entity = cx.weak_entity();
    let mut tools = div().flex().gap(px(5.0));
    for (tool, label) in [
        (SymbolTool::Line, "Line"),
        (SymbolTool::Rectangle, "Rectangle"),
        (SymbolTool::Ellipse, "Ellipse"),
        (SymbolTool::Text, "Text"),
    ] {
        tools = tools.child(
            div()
                .id(label)
                .px(px(8.0))
                .py(px(4.0))
                .cursor_pointer()
                .bg(rgb(if dialog.tool == tool {
                    palette.accent
                } else {
                    palette.chrome
                }))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(Dialog::Symbol(dialog)) = &mut this.dialog {
                        dialog.tool = tool;
                        dialog.start = None;
                    }
                    cx.notify();
                })),
        );
    }
    tools = tools
        .child(
            div()
                .id("symbol-delete-last")
                .cursor_pointer()
                .child("Undo shape")
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(Dialog::Symbol(dialog)) = &mut this.dialog {
                        dialog.shapes.pop();
                    }
                    cx.notify();
                })),
        )
        .child(
            div()
                .id("symbol-clear")
                .cursor_pointer()
                .child("Clear")
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(Dialog::Symbol(dialog)) = &mut this.dialog {
                        dialog.shapes.clear();
                    }
                    cx.notify();
                })),
        );
    div().flex().flex_col().gap(px(8.0)).child(tools).child("Drag to draw. Origin is the center; coordinates snap to 5. Text is placed by clicking.")
        .child(dialog.text.clone())
        .child(div().id("symbol-editor-canvas").debug_selector(||"symbol-editor-canvas".into()).h(px(320.0)).w_full().bg(rgb(palette.panel))
            .child(canvas(move|bounds,_,cx|{let _=entity.update(cx,|app,_|{if let Some(Dialog::Symbol(dialog))=&mut app.dialog{dialog.bounds=bounds;}});},move|bounds,_,window,cx|{
                let center=bounds.center();
                stroke([point(bounds.left(),center.y),point(bounds.right(),center.y)],1.0,rgb(palette.grid).into(),true,window);
                stroke([point(center.x,bounds.top()),point(center.x,bounds.bottom())],1.0,rgb(palette.grid).into(),true,window);
                paint_shapes(&shapes,|p|center+point(px(p.x*2.0),px(p.y*2.0)),2.0,palette.gate,palette.panel,window,cx);
            }).size_full())
            .on_mouse_down(MouseButton::Left,cx.listener(|this,event:&gpui::MouseDownEvent,_,cx|{
                if let Some(Dialog::Symbol(dialog))=&mut this.dialog {
                    let local=dialog.local(event.position);
                    if dialog.tool==SymbolTool::Text {dialog.shapes.push(SymbolPrimitive::Text {position:local,text:dialog.text.read(cx).value.clone()});}
                    else{dialog.start=Some(local);}
                }
                cx.stop_propagation();cx.notify();
            }))
            .on_mouse_up(MouseButton::Left,cx.listener(|this,event:&gpui::MouseUpEvent,_,cx|{
                if let Some(Dialog::Symbol(dialog))=&mut this.dialog && let Some(start)=dialog.start.take(){
                    let end=dialog.local(event.position);
                    if start!=end {dialog.shapes.push(match dialog.tool {SymbolTool::Line=>SymbolPrimitive::Line {start,end},SymbolTool::Ellipse=>SymbolPrimitive::Ellipse {start,end,filled:false},_=>SymbolPrimitive::Rectangle {start,end,filled:false}});}
                }
                cx.stop_propagation();cx.notify();
            })))
        .into_any_element()
}

pub fn paint_shapes(
    shapes: &[SymbolPrimitive],
    transform: impl Fn(Point) -> gpui::Point<Pixels>,
    zoom: f32,
    color: u32,
    background: u32,
    window: &mut Window,
    cx: &mut App,
) {
    for primitive in shapes {
        let bounds = primitive.bounds();
        match primitive {
            SymbolPrimitive::Line { start, end } => stroke(
                [transform(*start), transform(*end)],
                zoom,
                rgb(color).into(),
                false,
                window,
            ),
            SymbolPrimitive::Rectangle { filled, .. } => Shape::rounded_rect(
                bounds.min.x,
                bounds.min.y,
                bounds.size().x,
                bounds.size().y,
                0.0,
            )
            .paint(
                &transform,
                zoom,
                rgb(color).into(),
                filled.then(|| rgb(background).into()),
                window,
            ),
            SymbolPrimitive::Ellipse { filled, .. } => {
                let center = bounds.center();
                let half = bounds.size() / 2.0;
                let mut shape = Shape::default();
                for index in 0..=48 {
                    let angle = index as f32 * std::f32::consts::TAU / 48.0;
                    let p = center + Point::new(angle.cos() * half.x, angle.sin() * half.y);
                    if index == 0 {
                        shape.move_to(p.x, p.y);
                    } else {
                        shape.line_to(p.x, p.y);
                    }
                }
                shape.close();
                shape.paint(
                    &transform,
                    zoom,
                    rgb(color).into(),
                    filled.then(|| rgb(background).into()),
                    window,
                );
            }
            SymbolPrimitive::Text {
                position,
                text: label,
            } => text(
                label,
                transform(*position),
                10.0 * zoom,
                rgb(color).into(),
                window,
                cx,
            ),
        }
    }
}
