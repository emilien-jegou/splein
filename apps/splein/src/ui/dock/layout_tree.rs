// Declares the CSS flexbox tree in Taffy and computes resolved widget coordinates.

use super::geometry::{Box2D, DockGeometry};
use taffy::prelude::*;
use taffy::TaffyError;

pub fn compute_dock_layout(
    origin_x: f32,
    origin_y: f32,
    submenu_open: bool,
) -> Result<DockGeometry, TaffyError> {
    let mut taffy: TaffyTree<()> = TaffyTree::new();

    // 1. Drag Grip Handle (36x42)
    let grip = taffy.new_leaf(Style {
        size: Size {
            width: length(36.0f32),
            height: length(42.0f32),
        },
        ..Default::default()
    })?;

    // 2. 6 Tool Buttons (q: 46px, w: 42px, e: 42px, r: 46px, t: 42px, y: 42px)
    let widths = [46.0f32, 42.0f32, 42.0f32, 46.0f32, 42.0f32, 42.0f32];
    let tool_nodes: Vec<NodeId> = widths
        .iter()
        .map(|&w| {
            taffy.new_leaf(Style {
                size: Size {
                    width: length(w),
                    height: length(42.0f32),
                },
                ..Default::default()
            })
        })
        .collect::<Result<_, _>>()?;

    let tools_track = taffy.new_with_children(
        Style {
            display: Display::Flex,
            gap: Size {
                width: length(10.0f32),
                height: length(0.0f32),
            },
            ..Default::default()
        },
        &tool_nodes,
    )?;

    // 3. Hairline Divider (9x42)
    let divider = taffy.new_leaf(Style {
        size: Size {
            width: length(9.0f32),
            height: length(42.0f32),
        },
        ..Default::default()
    })?;

    // 4. Action Buttons (Trash: 42px, More: 42px, Gap: 4px)
    let act_0 = taffy.new_leaf(Style {
        size: Size {
            width: length(42.0f32),
            height: length(42.0f32),
        },
        ..Default::default()
    })?;
    let act_1 = taffy.new_leaf(Style {
        size: Size {
            width: length(42.0f32),
            height: length(42.0f32),
        },
        ..Default::default()
    })?;
    let actions_track = taffy.new_with_children(
        Style {
            display: Display::Flex,
            gap: Size {
                width: length(4.0f32),
                height: length(0.0f32),
            },
            ..Default::default()
        },
        &[act_0, act_1],
    )?;

    // 5. Main Dock Bar (padding: 3px 5px, gap: 6px)
    let root = taffy.new_with_children(
        Style {
            display: Display::Flex,
            align_items: Some(AlignItems::Center),
            padding: Rect {
                left: length(5.0f32),
                right: length(5.0f32),
                top: length(3.0f32),
                bottom: length(3.0f32),
            },
            gap: Size {
                width: length(6.0f32),
                height: length(0.0f32),
            },
            ..Default::default()
        },
        &[grip, tools_track, divider, actions_track],
    )?;

    taffy.compute_layout(root, Size::MAX_CONTENT)?;

    let to_box = |node: NodeId, ox: f32, oy: f32| -> Box2D {
        let l = taffy.layout(node).unwrap();
        Box2D::new(
            ox + l.location.x,
            oy + l.location.y,
            l.size.width,
            l.size.height,
        )
    };

    let container = to_box(root, origin_x, origin_y);
    let tools_ox = origin_x + taffy.layout(tools_track)?.location.x;
    let tools_oy = origin_y + taffy.layout(tools_track)?.location.y;

    let mut tools = [Box2D::default(); 6];
    for (i, &node) in tool_nodes.iter().enumerate() {
        tools[i] = to_box(node, tools_ox, tools_oy);
    }

    Ok(DockGeometry {
        container,
        grip: to_box(grip, origin_x, origin_y),
        tools,
        divider: to_box(divider, origin_x, origin_y),
        actions: [
            to_box(act_0, origin_x, origin_y),
            to_box(act_1, origin_x, origin_y),
        ],
        submenu: submenu_open.then(|| Box2D::new(origin_x + 117.5, origin_y + 61.0, 210.0, 47.0)),
        submenu_shapes: [
            Box2D::new(origin_x + 123.5, origin_y + 63.0, 42.0, 42.0),
            Box2D::new(origin_x + 175.5, origin_y + 63.0, 42.0, 42.0),
            Box2D::new(origin_x + 227.5, origin_y + 63.0, 42.0, 42.0),
            Box2D::new(origin_x + 279.5, origin_y + 63.0, 42.0, 42.0),
        ],
    })
}
