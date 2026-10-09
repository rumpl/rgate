use crate::{icons, vector::Shape};
use gpui::{AnyElement, canvas, point, prelude::*, px, rgb};

const INK: u32 = 0x303030;
const PAPER: u32 = 0xffffff;
const BLUE: u32 = 0x5555d9;
const LAVENDER: u32 = 0xaaaaf0;
const GOLD: u32 = 0xd7b65e;
const SILVER: u32 = 0xc0c0c0;
const RED: u32 = 0xdd3030;
const GREEN: u32 = 0x008b00;

struct Layer {
    shape: Shape,
    stroke: u32,
    fill: Option<u32>,
}

impl Layer {
    fn outline(shape: Shape, color: u32) -> Self {
        Self {
            shape,
            stroke: color,
            fill: None,
        }
    }

    fn filled(shape: Shape, stroke: u32, fill: u32) -> Self {
        Self {
            shape,
            stroke,
            fill: Some(fill),
        }
    }
}

fn polygon(points: &[(f32, f32)]) -> Shape {
    let mut shape = Shape::polyline(points);
    shape.close();
    shape
}

fn layers(name: &str) -> Vec<Layer> {
    let line = Shape::polyline;
    let rect = Shape::rounded_rect;
    let circle = Shape::circle;
    let outline = Layer::outline;
    let filled = Layer::filled;
    match name {
        "file_new" => vec![
            filled(
                polygon(&[(5., 2.), (14., 2.), (20., 8.), (20., 22.), (5., 22.)]),
                INK,
                PAPER,
            ),
            outline(line(&[(14., 2.), (14., 8.), (20., 8.)]), INK),
        ],
        "file_open" => vec![
            filled(
                polygon(&[
                    (3., 20.),
                    (3., 5.),
                    (10., 5.),
                    (12., 8.),
                    (21., 8.),
                    (21., 20.),
                ]),
                INK,
                SILVER,
            ),
            filled(
                polygon(&[(3., 20.), (7., 11.), (23., 11.), (19., 20.)]),
                INK,
                GOLD,
            ),
        ],
        "file_save" | "file_saveas" => {
            let mut result = vec![
                filled(
                    polygon(&[(3., 3.), (18., 3.), (21., 6.), (21., 21.), (3., 21.)]),
                    INK,
                    LAVENDER,
                ),
                filled(rect(7., 3., 10., 7., 0.), BLUE, PAPER),
                filled(rect(7., 14., 10., 7., 0.), BLUE, PAPER),
                outline(line(&[(9., 17.), (15., 17.)]), SILVER),
                outline(line(&[(14., 4.), (14., 8.)]), BLUE),
            ];
            if name == "file_saveas" {
                result.push(filled(
                    polygon(&[(4., 5.), (7., 3.), (20., 16.), (21., 20.), (17., 19.)]),
                    INK,
                    0xffdd22,
                ));
                result.push(outline(line(&[(16., 15.), (18., 13.)]), INK));
            }
            result
        }
        "sim_dump" => vec![
            filled(rect(15., 4., 6., 16., 0.), INK, SILVER),
            outline(line(&[(15., 8.), (21., 8.)]), INK),
            outline(line(&[(15., 12.), (21., 12.)]), INK),
            outline(line(&[(15., 16.), (21., 16.)]), INK),
            filled(
                polygon(&[
                    (12., 9.),
                    (7., 9.),
                    (7., 5.),
                    (1., 12.),
                    (7., 19.),
                    (7., 15.),
                    (12., 15.),
                ]),
                0x528e8e,
                0x88cccc,
            ),
        ],
        "back" | "forward" => {
            let mut arrow = Shape::default();
            arrow
                .move_to(3., 13.)
                .line_to(10., 6.)
                .line_to(10., 10.)
                .curve_to(21., 19., 23., 9.)
                .curve_to(11., 22., 21., 23.)
                .line_to(11., 18.)
                .curve_to(16., 16., 18., 19.)
                .line_to(10., 16.)
                .line_to(10., 20.)
                .close();
            // Reflect the path rather than substituting a different redo design.
            if name == "forward" {
                arrow = arrow.reflected_x(24.);
            }
            vec![filled(arrow, BLUE, LAVENDER)]
        }
        "edit_cut" => vec![
            filled(
                polygon(&[(5., 3.), (7., 3.), (18., 17.), (16., 19.)]),
                INK,
                SILVER,
            ),
            filled(
                polygon(&[(19., 3.), (17., 3.), (6., 17.), (8., 19.)]),
                INK,
                SILVER,
            ),
            filled(circle(6., 19., 3.), BLUE, PAPER),
            filled(circle(18., 19., 3.), BLUE, PAPER),
            filled(circle(12., 12., 1.), INK, SILVER),
        ],
        "edit_copy" => vec![
            filled(rect(10., 3., 10., 13., 0.), INK, PAPER),
            filled(rect(4., 9., 10., 13., 0.), INK, PAPER),
            outline(line(&[(7., 13.), (11., 13.)]), INK),
            outline(line(&[(7., 17.), (11., 17.)]), INK),
        ],
        "edit_paste" => vec![
            filled(rect(4., 4., 15., 18., 0.), INK, GOLD),
            filled(rect(8., 2., 7., 5., 0.), INK, SILVER),
            filled(rect(10., 9., 11., 13., 0.), INK, PAPER),
            outline(line(&[(13., 13.), (18., 13.)]), INK),
            outline(line(&[(13., 17.), (18., 17.)]), INK),
        ],
        "mov_curs" | "editmode" => vec![filled(
            polygon(&[
                (4., 2.),
                (4., 20.),
                (9., 15.),
                (14., 23.),
                (17., 21.),
                (12., 13.),
                (20., 13.),
            ]),
            INK,
            0x101010,
        )],
        "net_wire" => vec![
            filled(rect(2., 3., 20., 19., 0.), 0x888888, PAPER),
            outline(line(&[(5., 6.), (18., 6.)]), BLUE),
            outline(line(&[(12., 6.), (12., 17.), (7., 17.)]), GREEN),
            filled(circle(12., 6., 1.8), BLUE, BLUE),
            outline(line(&[(6., 20.), (9., 12.), (12., 20.)]), INK),
            outline(line(&[(7., 17.), (11., 17.)]), INK),
        ],
        "scroll_curs" => vec![filled(icons::shapes(name).remove(0), INK, PAPER)],
        "cut_curs" => vec![
            outline(line(&[(4., 3.), (12., 10.), (20., 22.)]), INK),
            outline(
                line(&[(8., 3.), (15., 9.), (13., 14.), (7., 12.), (3., 7.)]),
                INK,
            ),
            filled(
                polygon(&[(12., 10.), (15., 8.), (22., 19.), (19., 21.)]),
                INK,
                SILVER,
            ),
        ],
        "edit_rotate" | "edit_brotate" => {
            let mut result = vec![
                outline(line(&[(5., 2.), (5., 22.), (22., 22.)]), GREEN),
                filled(rect(3., 6., 7., 10., 0.), BLUE, PAPER),
                filled(rect(10., 18., 10., 5., 0.), BLUE, PAPER),
            ];
            let arrow = polygon(&[
                (10., 5.),
                (16., 5.),
                (16., 2.),
                (22., 8.),
                (16., 14.),
                (16., 11.),
                (13., 11.),
                (13., 14.),
                (10., 14.),
            ]);
            result.push(filled(
                if name == "edit_brotate" {
                    arrow.reflected_x(24.)
                } else {
                    arrow
                },
                RED,
                RED,
            ));
            result
        }
        "gateprops" => vec![
            outline(line(&[(5., 5.), (19., 8.)]), RED),
            outline(line(&[(4., 10.), (20., 13.)]), RED),
            outline(line(&[(3., 15.), (19., 18.)]), RED),
            filled(
                polygon(&[(8., 3.), (17., 5.), (13., 22.), (4., 20.)]),
                BLUE,
                PAPER,
            ),
        ],
        "sim_go" | "simulate" => vec![filled(
            polygon(&[(5., 3.), (21., 12.), (5., 21.)]),
            0x666666,
            PAPER,
        )],
        "sim_pause" => vec![
            filled(rect(5., 3., 5., 18., 0.), 0x666666, PAPER),
            filled(rect(14., 3., 5., 18., 0.), 0x666666, PAPER),
        ],
        "sim_stop" => vec![filled(rect(4., 4., 16., 16., 0.), 0x666666, PAPER)],
        "sim_step" => vec![
            filled(polygon(&[(3., 4.), (14., 12.), (3., 20.)]), INK, SILVER),
            filled(rect(17., 4., 4., 16., 0.), INK, SILVER),
        ],
        "sim_clock" => vec![
            outline(line(&[(4., 22.), (17., 9.)]), INK),
            filled(circle(13., 9., 7.), INK, PAPER),
            outline(line(&[(13., 4.), (13., 9.), (17., 9.)]), RED),
        ],
        "zoom_in" | "zoom_out" => {
            let mut result = vec![
                filled(
                    polygon(&[(13., 14.), (16., 12.), (23., 21.), (20., 23.)]),
                    INK,
                    GOLD,
                ),
                filled(circle(9., 9., 7.), INK, LAVENDER),
                outline(line(&[(5., 9.), (13., 9.)]), INK),
            ];
            if name == "zoom_in" {
                result.push(outline(line(&[(9., 5.), (9., 13.)]), INK));
            }
            result
        }
        _ => icons::shapes(name)
            .into_iter()
            .map(|shape| {
                outline(
                    shape,
                    if matches!(name, "output" | "sim_view") {
                        GREEN
                    } else {
                        BLUE
                    },
                )
            })
            .collect(),
    }
}

pub fn icon(name: &str, dimension: f32) -> AnyElement {
    debug_assert!(icons::NAMES.contains(&name));
    let layers = layers(name);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let scale = f32::from(bounds.size.width.min(bounds.size.height)) / 24.;
            for layer in &layers {
                layer.shape.paint(
                    |p| bounds.origin + point(px(p.x * scale), px(p.y * scale)),
                    1.5 * scale,
                    rgb(layer.stroke).into(),
                    layer.fill.map(|color| rgb(color).into()),
                    window,
                );
            }
        },
    )
    .size(px(dimension))
    .flex_none()
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_toolbar_and_sidebar_icon_has_classic_vector_artwork() {
        for name in icons::NAMES {
            assert!(!layers(name).is_empty(), "{name}");
        }
    }

    #[test]
    fn classic_toolbar_keeps_filled_multicolor_icons() {
        for name in [
            "file_open",
            "file_save",
            "back",
            "edit_cut",
            "edit_paste",
            "edit_rotate",
            "zoom_in",
        ] {
            let layers = layers(name);
            assert!(
                layers.iter().any(|layer| layer.fill.is_some()),
                "{name} has no fill"
            );
            assert!(
                layers.iter().any(|layer| layer.stroke != INK
                    || layer
                        .fill
                        .is_some_and(|color| color != PAPER && color != INK)),
                "{name} lost its classic colors"
            );
        }
    }
}
