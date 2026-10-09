use crate::{theme::Palette, vector::Shape};
use gpui::{AnyElement, canvas, point, prelude::*, px, rgb};

pub const NAMES: &[&str] = &[
    "file_new",
    "file_open",
    "file_save",
    "file_saveas",
    "sim_dump",
    "back",
    "forward",
    "edit_cut",
    "edit_copy",
    "edit_paste",
    "mov_curs",
    "net_wire",
    "scroll_curs",
    "cut_curs",
    "edit_rotate",
    "edit_brotate",
    "gateprops",
    "sim_go",
    "sim_pause",
    "sim_stop",
    "sim_step",
    "sim_clock",
    "zoom_in",
    "zoom_out",
    "parts",
    "module_root",
    "mod_net",
    "output",
    "modtree",
    "modlist",
    "editmode",
    "iface",
    "simulate",
    "log",
    "sim_view",
    "gatelogo",
];

pub fn shapes(name: &str) -> Vec<Shape> {
    let line = Shape::polyline;
    let rect = Shape::rounded_rect;
    let circle = Shape::circle;
    match name {
        "file_new" => vec![
            line(&[
                (14., 3.),
                (5., 3.),
                (5., 21.),
                (19., 21.),
                (19., 8.),
                (14., 3.),
                (14., 8.),
                (19., 8.),
            ]),
            line(&[(8., 14.), (16., 14.)]),
            line(&[(12., 10.), (12., 18.)]),
        ],
        "file_open" => vec![
            line(&[
                (3., 19.),
                (3., 5.),
                (9., 5.),
                (12., 8.),
                (21., 8.),
                (21., 11.),
            ]),
            line(&[(3., 19.), (7., 11.), (22., 11.), (18., 19.), (3., 19.)]),
        ],
        "file_save" => vec![
            rect(4., 3., 16., 18., 2.),
            rect(8., 3., 8., 6., 0.),
            rect(8., 14., 8., 7., 1.),
        ],
        "file_saveas" => vec![
            rect(3., 3., 15., 18., 2.),
            rect(7., 3., 7., 6., 0.),
            line(&[
                (12., 19.),
                (20., 11.),
                (23., 14.),
                (15., 22.),
                (11., 23.),
                (12., 19.),
            ]),
        ],
        "sim_dump" => vec![
            line(&[(5., 9.), (5., 21.), (19., 21.), (19., 9.)]),
            line(&[(12., 15.), (12., 3.)]),
            line(&[(8., 7.), (12., 3.), (16., 7.)]),
        ],
        "back" | "forward" => {
            let mut arc = Shape::default();
            arc.move_to(5., 8.).curve_to(20., 17., 20., 3.);
            if name == "back" {
                vec![arc, line(&[(5., 3.), (5., 8.), (10., 8.)])]
            } else {
                vec![
                    line(&[(4., 17.), (4., 10.), (10., 7.), (19., 8.), (19., 3.)]),
                    line(&[(14., 8.), (19., 8.), (19., 3.)]),
                ]
            }
        }
        "edit_cut" => vec![
            circle(6., 17., 3.),
            circle(18., 17., 3.),
            line(&[(8., 15.), (19., 3.)]),
            line(&[(16., 15.), (5., 3.)]),
        ],
        "edit_copy" => vec![
            rect(8., 8., 13., 13., 2.),
            line(&[(16., 5.), (16., 3.), (3., 3.), (3., 16.), (5., 16.)]),
        ],
        "edit_paste" => vec![
            rect(5., 5., 14., 16., 2.),
            rect(9., 3., 6., 4., 1.),
            line(&[(9., 12.), (15., 12.)]),
            line(&[(9., 16.), (15., 16.)]),
        ],
        "mov_curs" | "editmode" => vec![line(&[
            (5., 3.),
            (5., 20.),
            (10., 15.),
            (14., 22.),
            (17., 20.),
            (13., 13.),
            (20., 13.),
            (5., 3.),
        ])],
        "net_wire" => vec![
            circle(4., 18., 2.),
            circle(20., 6., 2.),
            line(&[(6., 18.), (12., 18.), (12., 6.), (18., 6.)]),
        ],
        "scroll_curs" => vec![line(&[
            (8., 20.),
            (4., 12.),
            (6., 10.),
            (9., 13.),
            (9., 4.),
            (12., 4.),
            (12., 11.),
            (15., 6.),
            (18., 8.),
            (21., 11.),
            (19., 20.),
            (8., 20.),
        ])],
        "cut_curs" => vec![
            line(&[(5., 7.), (19., 7.)]),
            rect(7., 7., 10., 14., 2.),
            line(&[(9., 7.), (9., 3.), (15., 3.), (15., 7.)]),
            line(&[(10., 11.), (10., 17.)]),
            line(&[(14., 11.), (14., 17.)]),
        ],
        "edit_rotate" | "edit_brotate" => vec![
            line(&[
                (19., 8.),
                (16., 4.),
                (8., 4.),
                (4., 8.),
                (4., 16.),
                (8., 20.),
                (16., 20.),
                (20., 16.),
            ]),
            line(if name == "edit_rotate" {
                &[(14., 8.), (19., 8.), (19., 3.)]
            } else {
                &[(4., 3.), (4., 8.), (9., 8.)]
            }),
        ],
        "gateprops" => vec![
            line(&[(4., 6.), (20., 6.)]),
            line(&[(4., 12.), (20., 12.)]),
            line(&[(4., 18.), (20., 18.)]),
            circle(9., 6., 2.),
            circle(15., 12., 2.),
            circle(9., 18., 2.),
        ],
        "sim_go" | "simulate" => vec![line(&[(7., 3.), (21., 12.), (7., 21.), (7., 3.)])],
        "sim_pause" => vec![rect(6., 4., 4., 16., 1.), rect(14., 4., 4., 16., 1.)],
        "sim_stop" => vec![rect(5., 5., 14., 14., 2.)],
        "sim_step" => vec![
            line(&[(4., 4.), (16., 12.), (4., 20.), (4., 4.)]),
            line(&[(20., 4.), (20., 20.)]),
        ],
        "sim_clock" => vec![
            circle(12., 12., 9.),
            line(&[(12., 6.), (12., 12.), (16., 15.)]),
        ],
        "zoom_in" | "zoom_out" => {
            let mut result = vec![
                circle(10., 10., 7.),
                line(&[(15., 15.), (21., 21.)]),
                line(&[(6., 10.), (14., 10.)]),
            ];
            if name == "zoom_in" {
                result.push(line(&[(10., 6.), (10., 14.)]));
            }
            result
        }
        "parts" | "iface" | "gatelogo" => vec![
            rect(6., 6., 12., 12., 3.),
            line(&[(2., 9.), (6., 9.)]),
            line(&[(2., 15.), (6., 15.)]),
            line(&[(18., 9.), (22., 9.)]),
            line(&[(18., 15.), (22., 15.)]),
            line(&[(9., 2.), (9., 6.)]),
            line(&[(15., 18.), (15., 22.)]),
        ],
        "module_root" | "mod_net" => vec![
            rect(4., 4., 16., 16., 3.),
            line(&[(1., 9.), (4., 9.)]),
            line(&[(20., 15.), (23., 15.)]),
            rect(9., 9., 6., 6., 1.),
        ],
        "output" => vec![
            line(&[(3., 12.), (21., 12.)]),
            line(&[(15., 6.), (21., 12.), (15., 18.)]),
        ],
        "modtree" => vec![
            rect(9., 2., 6., 6., 1.),
            rect(2., 16., 6., 6., 1.),
            rect(16., 16., 6., 6., 1.),
            line(&[(12., 8.), (12., 12.), (5., 12.), (5., 16.)]),
            line(&[(12., 12.), (19., 12.), (19., 16.)]),
        ],
        "modlist" | "log" => vec![
            line(&[(8., 5.), (21., 5.)]),
            line(&[(8., 12.), (21., 12.)]),
            line(&[(8., 19.), (21., 19.)]),
            circle(3., 5., 1.),
            circle(3., 12., 1.),
            circle(3., 19., 1.),
        ],
        "sim_view" => vec![
            line(&[(3., 3.), (3., 21.), (22., 21.)]),
            line(&[
                (5., 15.),
                (9., 15.),
                (9., 7.),
                (15., 7.),
                (15., 15.),
                (21., 15.),
            ]),
        ],
        _ => panic!("missing modern icon: {name}"),
    }
}

pub fn icon(name: &str, palette: Palette, dimension: f32) -> AnyElement {
    debug_assert!(NAMES.contains(&name));
    let shapes = shapes(name);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let scale = f32::from(bounds.size.width.min(bounds.size.height)) / 24.0;
            for shape in &shapes {
                shape.paint(
                    |p| bounds.origin + point(px(p.x * scale), px(p.y * scale)),
                    1.7 * scale,
                    rgb(palette.text).into(),
                    None,
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
    fn every_modern_icon_has_vector_artwork() {
        for name in NAMES {
            assert!(!shapes(name).is_empty(), "{name}");
        }
    }
}
