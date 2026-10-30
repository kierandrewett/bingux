//! `Node::rtl`: the children of a mirrored Node are laid out as usual and then
//! reflected about the Node's box.

use torin::{
    prelude::*,
    test_utils::*,
};

fn px(value: f32) -> Size {
    Size::Pixels(Length::new(value))
}

fn at(layout: &Torin<usize>, id: usize) -> (f32, f32, f32, f32) {
    let area = layout.get(&id).unwrap().area;
    (area.min_x(), area.min_y(), area.width(), area.height())
}

fn measure(layout: &mut Torin<usize>, tree: &TestingTree) {
    layout.measure(
        0,
        Rect::new(Point2D::new(0.0, 0.0), Size2D::new(1000.0, 1000.0)),
        &mut None::<NoopMeasurer>,
        tree,
    );
}

/// A 300 x 100 row of three 50 px boxes with 10 px left and 4 px right padding.
fn row(rtl: bool) -> TestingTree {
    let mut tree = TestingTree::default();
    tree.add(
        0,
        None,
        vec![1, 2, 3],
        Node {
            rtl,
            padding: Gaps::new(0.0, 4.0, 0.0, 10.0),
            ..Node::from_size_and_direction(px(300.0), px(100.0), Direction::Horizontal)
        },
    );
    for id in 1..=3 {
        tree.add(
            id,
            Some(0),
            vec![],
            Node::from_size_and_direction(px(50.0), px(20.0), Direction::Vertical),
        );
    }
    tree
}

#[test]
fn a_row_runs_from_the_right_and_its_start_padding_is_on_the_right() {
    let (mut layout, _) = test_utils();
    measure(&mut layout, &row(false));
    assert_eq!(at(&layout, 1), (10.0, 0.0, 50.0, 20.0));
    assert_eq!(at(&layout, 3), (110.0, 0.0, 50.0, 20.0));

    let (mut layout, _) = test_utils();
    measure(&mut layout, &row(true));
    // The reflection of [10, 60] in [0, 300] is [240, 290]: the 10 px of start
    // padding is now on the right, the 4 px of end padding on the left.
    assert_eq!(at(&layout, 1), (240.0, 0.0, 50.0, 20.0));
    assert_eq!(at(&layout, 2), (190.0, 0.0, 50.0, 20.0));
    assert_eq!(at(&layout, 3), (140.0, 0.0, 50.0, 20.0));
}

#[test]
fn a_column_aligns_its_children_to_the_start_edge_which_is_the_right() {
    let mut tree = TestingTree::default();
    tree.add(
        0,
        None,
        vec![1, 2],
        Node {
            rtl: true,
            ..Node::from_size_and_direction(px(200.0), px(100.0), Direction::Vertical)
        },
    );
    tree.add(1, Some(0), vec![], Node::from_size_and_direction(px(80.0), px(20.0), Direction::Vertical));
    tree.add(2, Some(0), vec![], Node::from_size_and_direction(px(30.0), px(20.0), Direction::Vertical));
    let (mut layout, _) = test_utils();
    measure(&mut layout, &tree);
    assert_eq!(at(&layout, 1), (120.0, 0.0, 80.0, 20.0));
    assert_eq!(at(&layout, 2), (170.0, 20.0, 30.0, 20.0));
}

#[test]
fn an_end_alignment_ends_at_the_left() {
    let mut tree = TestingTree::default();
    tree.add(
        0,
        None,
        vec![1],
        Node {
            rtl: true,
            ..Node::from_size_and_alignments_and_direction(
                px(200.0),
                px(100.0),
                Alignment::End,
                Alignment::Start,
                Direction::Horizontal,
            )
        },
    );
    tree.add(1, Some(0), vec![], Node::from_size_and_direction(px(50.0), px(20.0), Direction::Vertical));
    let (mut layout, _) = test_utils();
    measure(&mut layout, &tree);
    assert_eq!(at(&layout, 1), (0.0, 0.0, 50.0, 20.0));
}

#[test]
fn each_level_mirrors_its_own_children_and_the_box_moves_whole() {
    let mut tree = TestingTree::default();
    tree.add(
        0,
        None,
        vec![1],
        Node {
            rtl: true,
            ..Node::from_size_and_direction(px(300.0), px(100.0), Direction::Horizontal)
        },
    );
    tree.add(
        1,
        Some(0),
        vec![2, 3],
        Node {
            rtl: true,
            ..Node::from_size_and_direction(px(100.0), px(50.0), Direction::Horizontal)
        },
    );
    tree.add(2, Some(1), vec![], Node::from_size_and_direction(px(30.0), px(20.0), Direction::Vertical));
    tree.add(3, Some(1), vec![], Node::from_size_and_direction(px(30.0), px(20.0), Direction::Vertical));
    let (mut layout, _) = test_utils();
    measure(&mut layout, &tree);
    // The box that was at 0..100 is at 200..300; inside it the first child is
    // at its right end, 270..300, the second to its left.
    assert_eq!(at(&layout, 1), (200.0, 0.0, 100.0, 50.0));
    assert_eq!(at(&layout, 2), (270.0, 0.0, 30.0, 20.0));
    assert_eq!(at(&layout, 3), (240.0, 0.0, 30.0, 20.0));
}

#[test]
fn a_left_to_right_box_inside_a_mirrored_one_keeps_its_own_order() {
    let mut tree = TestingTree::default();
    tree.add(
        0,
        None,
        vec![1],
        Node {
            rtl: true,
            ..Node::from_size_and_direction(px(300.0), px(100.0), Direction::Horizontal)
        },
    );
    tree.add(
        1,
        Some(0),
        vec![2, 3],
        Node::from_size_and_direction(px(100.0), px(50.0), Direction::Horizontal),
    );
    tree.add(2, Some(1), vec![], Node::from_size_and_direction(px(30.0), px(20.0), Direction::Vertical));
    tree.add(3, Some(1), vec![], Node::from_size_and_direction(px(30.0), px(20.0), Direction::Vertical));
    let (mut layout, _) = test_utils();
    measure(&mut layout, &tree);
    assert_eq!(at(&layout, 1), (200.0, 0.0, 100.0, 50.0));
    assert_eq!(at(&layout, 2), (200.0, 0.0, 30.0, 20.0));
    assert_eq!(at(&layout, 3), (230.0, 0.0, 30.0, 20.0));
}

#[test]
fn the_margin_of_the_mirrored_box_is_not_part_of_what_is_mirrored() {
    let mut tree = TestingTree::default();
    tree.add(
        0,
        None,
        vec![1],
        Node::from_size_and_direction(px(400.0), px(100.0), Direction::Horizontal),
    );
    tree.add(
        1,
        Some(0),
        vec![2],
        Node {
            rtl: true,
            margin: Gaps::new(0.0, 0.0, 0.0, 40.0),
            ..Node::from_size_and_direction(px(200.0), px(50.0), Direction::Horizontal)
        },
    );
    tree.add(2, Some(1), vec![], Node::from_size_and_direction(px(50.0), px(20.0), Direction::Vertical));
    let (mut layout, _) = test_utils();
    measure(&mut layout, &tree);
    // The box is 40..240 (the margin is on its left); its child sits at the
    // box's right end.
    assert_eq!(at(&layout, 2).0, 190.0);
}

#[test]
fn an_absolute_left_becomes_a_right_and_a_global_position_stays() {
    let mut tree = TestingTree::default();
    tree.add(
        0,
        None,
        vec![1, 2],
        Node {
            rtl: true,
            ..Node::from_size_and_direction(px(300.0), px(100.0), Direction::Vertical)
        },
    );
    tree.add(
        1,
        Some(0),
        vec![],
        Node::from_size_and_position(px(40.0), px(10.0), Position::new_absolute().left(20.0).top(5.0)),
    );
    tree.add(
        2,
        Some(0),
        vec![],
        Node::from_size_and_position(px(40.0), px(10.0), Position::new_global().left(20.0).top(5.0)),
    );
    let (mut layout, _) = test_utils();
    measure(&mut layout, &tree);
    assert_eq!(at(&layout, 1).0, 240.0);
    assert_eq!(at(&layout, 2).0, 20.0);
}

#[test]
fn a_scrolled_offset_keeps_its_sign_and_moves_the_content_forward() {
    let content = |offset: f32| {
        let mut tree = TestingTree::default();
        tree.add(
            0,
            None,
            vec![1],
            Node {
                rtl: true,
                offset_x: Length::new(offset),
                ..Node::from_size_and_direction(px(100.0), px(50.0), Direction::Vertical)
            },
        );
        tree.add(
            1,
            Some(0),
            vec![],
            Node::from_size_and_direction(px(300.0), px(50.0), Direction::Vertical),
        );
        tree
    };
    let (mut layout, _) = test_utils();
    measure(&mut layout, &content(0.0));
    // At rest the content's start is at the right edge and it overflows left.
    assert_eq!(at(&layout, 1).0, -200.0);
    let (mut layout, _) = test_utils();
    measure(&mut layout, &content(-200.0));
    // Scrolled to the end (a negative offset, as in a left to right Node): its
    // far end is at the viewport's left edge.
    assert_eq!(at(&layout, 1).0, 0.0);
}

#[test]
fn a_changed_child_is_mirrored_again_and_matches_a_fresh_layout() {
    let build = |width: f32| {
        let mut tree = row(true);
        tree.set_node(
            2,
            Node::from_size_and_direction(px(width), px(20.0), Direction::Vertical),
        );
        tree
    };
    let (mut layout, _) = test_utils();
    measure(&mut layout, &build(50.0));
    let changed = build(80.0);
    layout.invalidate(2);
    layout.invalidate(0);
    measure(&mut layout, &changed);
    let (mut fresh, _) = test_utils();
    measure(&mut fresh, &changed);
    for id in 0..=3 {
        assert_eq!(at(&layout, id), at(&fresh, id), "node {id}");
    }
    assert_eq!(at(&fresh, 2), (160.0, 0.0, 80.0, 20.0));
}

#[test]
fn scrolling_without_a_relayout_moves_the_content_the_same_way() {
    let node = |offset: f32| Node {
        rtl: true,
        offset_x: Length::new(offset),
        ..Node::from_size_and_direction(px(100.0), px(50.0), Direction::Vertical)
    };
    let mut tree = TestingTree::default();
    tree.add(0, None, vec![1], node(0.0));
    tree.add(1, Some(0), vec![], Node::from_size_and_direction(px(300.0), px(50.0), Direction::Vertical));
    let (mut layout, _) = test_utils();
    measure(&mut layout, &tree);
    assert_eq!(at(&layout, 1).0, -200.0);
    tree.set_node(0, node(-150.0));
    layout.invalidate_with_reason(0, DirtyReason::InnerLayout);
    measure(&mut layout, &tree);
    assert_eq!(at(&layout, 1).0, -50.0);
}

#[test]
fn a_childs_start_margin_moves_to_the_other_side_with_it() {
    let mut tree = TestingTree::default();
    tree.add(
        0,
        None,
        vec![1],
        Node {
            rtl: true,
            ..Node::from_size_and_direction(px(200.0), px(100.0), Direction::Vertical)
        },
    );
    tree.add(
        1,
        Some(0),
        vec![],
        Node {
            margin: Gaps::new(0.0, 10.0, 0.0, 12.0),
            ..Node::from_size_and_direction(px(120.0), px(20.0), Direction::Vertical)
        },
    );
    let (mut layout, _) = test_utils();
    measure(&mut layout, &tree);
    // Left to right the box is at 12..132; its reflection is at 68..188.
    let area = layout.get(&1).unwrap().area;
    assert_eq!(area.min_x() + 12.0, 68.0);
}

/// A region that clips a scroller that holds a row of absolutely placed tabs,
/// the shape of a strip of tabs: the region narrows and the content scrolls.
fn strip(region: f32, offset: f32) -> TestingTree {
    let mut tree = TestingTree::default();
    tree.add(
        0,
        None,
        vec![1],
        Node {
            rtl: true,
            ..Node::from_size_and_direction(px(region), px(44.0), Direction::Horizontal)
        },
    );
    tree.add(
        1,
        Some(0),
        vec![2],
        Node {
            rtl: true,
            offset_x: Length::new(-offset),
            ..Node::from_size_and_direction(px(392.0), px(44.0), Direction::Vertical)
        },
    );
    tree.add(
        2,
        Some(1),
        vec![3, 4, 5],
        Node {
            rtl: true,
            ..Node::from_size_and_direction(px(392.0), px(44.0), Direction::Vertical)
        },
    );
    for (id, left) in [(3, 0.0), (4, 132.0), (5, 264.0)] {
        tree.add(
            id,
            Some(2),
            vec![],
            Node::from_size_and_position(px(128.0), px(44.0), Position::new_absolute().left(left).top(0.0)),
        );
    }
    tree
}

#[test]
fn a_scrolled_node_in_a_mirrored_parent_is_not_mirrored_again_and_matches_a_fresh_layout() {
    // The parent is resized and laid out, then the child, which is not dirty
    // itself but has an offset change (`InnerLayout`), keeps the place its parent
    // gave it while its content moves: the same as a layout from scratch, at each
    // step, however the offset changes.
    let (mut layout, _) = test_utils();
    measure(&mut layout, &strip(632.0, 0.0));
    let tree = strip(202.0, 0.0);
    layout.invalidate(0);
    measure(&mut layout, &tree);
    for offset in [24.0, 68.0, 0.0, 190.0] {
        let tree = strip(202.0, offset);
        layout.invalidate_with_reason(1, DirtyReason::InnerLayout);
        measure(&mut layout, &tree);
        let (mut fresh, _) = test_utils();
        measure(&mut fresh, &tree);
        for id in 0..=5 {
            assert_eq!(at(&layout, id), at(&fresh, id), "node {id} at offset {offset}");
        }
    }
    // The content really moved: at the end of its range the last tab is at the left edge.
    assert_eq!(at(&layout, 5).0, 0.0);
}

#[test]
fn a_node_measured_as_the_root_of_a_relayout_is_mirrored_in_its_mirrored_parent() {
    // The parent is laid out and then only the child (a changed width) is
    // measured again, as a relayout root: it is placed from the parent's left and
    // mirrored, the same as a layout from scratch.
    let build = |width: f32| {
        let mut tree = strip(202.0, 0.0);
        tree.set_node(
            1,
            Node {
                rtl: true,
                ..Node::from_size_and_direction(px(width), px(44.0), Direction::Vertical)
            },
        );
        tree
    };
    let (mut layout, _) = test_utils();
    measure(&mut layout, &build(392.0));
    let tree = build(300.0);
    layout.invalidate(1);
    layout.root_node_candidate = RootNodeCandidate::Valid(1);
    measure(&mut layout, &tree);
    let (mut fresh, _) = test_utils();
    measure(&mut fresh, &tree);
    for id in 0..=5 {
        assert_eq!(at(&layout, id), at(&fresh, id), "node {id}");
    }
}

/// A header bar with a strip of tabs in a mirrored window: the bar (padded at its
/// start), a container, the strip (a row of the scrolling region and the buttons),
/// the region that clips the scroller, the scroller (offset) and the row of tabs.
fn window_strip(offset: f32) -> TestingTree {
    let mut tree = TestingTree::default();
    let rtl = |node: Node| Node { rtl: true, ..node };
    tree.add(0, None, vec![1], rtl(Node::from_size_and_direction(px(390.0), px(100.0), Direction::Vertical)));
    tree.add(1, Some(0), vec![2], rtl(Node { padding: Gaps::new(0.0, 0.0, 0.0, 8.0), ..Node::from_size_and_direction(px(390.0), px(44.0), Direction::Horizontal) }));
    tree.add(2, Some(1), vec![3], rtl(Node::from_size_and_direction(px(280.0), px(44.0), Direction::Horizontal)));
    tree.add(3, Some(2), vec![4, 10], rtl(Node::from_size_and_direction(px(280.0), px(44.0), Direction::Horizontal)));
    tree.add(4, Some(3), vec![5], rtl(Node::from_size_and_direction(px(192.0), px(44.0), Direction::Horizontal)));
    tree.add(5, Some(4), vec![6], rtl(Node { offset_x: Length::new(-offset), ..Node::from_size_and_direction(px(392.0), px(44.0), Direction::Horizontal) }));
    tree.add(6, Some(5), vec![7, 8, 9], rtl(Node::from_size_and_direction(px(392.0), px(44.0), Direction::Horizontal)));
    for (id, left) in [(7, 0.0), (8, 132.0), (9, 264.0)] {
        tree.add(id, Some(6), vec![], Node::from_size_and_position(px(128.0), px(44.0), Position::new_absolute().left(left).top(0.0)));
    }
    tree.add(10, Some(3), vec![], rtl(Node::from_size_and_direction(px(88.0), px(44.0), Direction::Horizontal)));
    tree
}

#[test]
fn a_subtree_measured_again_from_a_root_below_a_mirrored_parent_matches_a_fresh_layout() {
    let (mut layout, _) = test_utils();
    measure(&mut layout, &window_strip(0.0));
    // The strip's container and everything under it are dirty after the scroll (the
    // tabs are drawn again), and the container is the root of the relayout.
    let tree = window_strip(68.0);
    // The scroller's own change is only its offset, which would let it be moved
    // where it is; but its parent is measured again, and places it again.
    for id in (2..=10).filter(|id| *id != 5) {
        layout.invalidate(id);
    }
    layout.invalidate_with_reason(5, DirtyReason::InnerLayout);
    layout.root_node_candidate = RootNodeCandidate::Valid(2);
    measure(&mut layout, &tree);
    let (mut fresh, _) = test_utils();
    measure(&mut fresh, &tree);
    for id in 0..=10 {
        assert_eq!(at(&layout, id), at(&fresh, id), "node {id}");
    }
    assert_eq!(at(&layout, 5).0, -10.0, "the scroller is right aligned in its region");
    assert_eq!(at(&layout, 7).0, 322.0);
}
