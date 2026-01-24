use crate::tui::oil::node::Size;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildMeasurement {
    Fixed(usize),
    Flex(u16),
    Content(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlexLayoutInput {
    pub available: usize,
    pub children: Vec<ChildMeasurement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlexLayoutResult {
    pub widths: Vec<usize>,
    pub total_used: usize,
}

pub fn calculate_row_widths(input: &FlexLayoutInput) -> FlexLayoutResult {
    if input.children.is_empty() {
        return FlexLayoutResult {
            widths: vec![],
            total_used: 0,
        };
    }

    let mut fixed_used: usize = 0;
    let mut total_flex_weight: u16 = 0;

    for child in &input.children {
        match child {
            ChildMeasurement::Fixed(w) | ChildMeasurement::Content(w) => {
                fixed_used += w;
            }
            ChildMeasurement::Flex(weight) => {
                total_flex_weight += weight;
            }
        }
    }

    let remaining = input.available.saturating_sub(fixed_used);

    let mut widths = Vec::with_capacity(input.children.len());
    let mut total_used = 0;

    for child in &input.children {
        let width = match child {
            ChildMeasurement::Fixed(w) | ChildMeasurement::Content(w) => *w,
            ChildMeasurement::Flex(weight) => {
                if total_flex_weight > 0 {
                    (remaining as u32 * *weight as u32 / total_flex_weight as u32) as usize
                } else {
                    0
                }
            }
        };
        widths.push(width);
        total_used += width;
    }

    FlexLayoutResult { widths, total_used }
}

pub fn calculate_column_heights(
    available_height: usize,
    children: &[ChildMeasurement],
    gap: usize,
) -> Vec<usize> {
    if children.is_empty() {
        return vec![];
    }

    let mut fixed_used: usize = 0;
    let mut total_flex_weight: u16 = 0;
    let total_gap = gap * children.len().saturating_sub(1);

    for child in children {
        match child {
            ChildMeasurement::Fixed(h) | ChildMeasurement::Content(h) => {
                fixed_used += h;
            }
            ChildMeasurement::Flex(weight) => {
                total_flex_weight += weight;
            }
        }
    }

    let remaining = available_height.saturating_sub(fixed_used + total_gap);

    children
        .iter()
        .map(|child| match child {
            ChildMeasurement::Fixed(h) | ChildMeasurement::Content(h) => *h,
            ChildMeasurement::Flex(weight) => {
                if total_flex_weight > 0 {
                    (remaining as u32 * *weight as u32 / total_flex_weight as u32) as usize
                } else {
                    0
                }
            }
        })
        .collect()
}

pub fn size_to_measurement(size: Size, content_size: usize) -> ChildMeasurement {
    match size {
        Size::Fixed(w) => ChildMeasurement::Fixed(w as usize),
        Size::Flex(weight) => ChildMeasurement::Flex(weight),
        Size::Content => ChildMeasurement::Content(content_size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_child_measurement() -> impl Strategy<Value = ChildMeasurement> {
        prop_oneof![
            (1usize..200).prop_map(ChildMeasurement::Fixed),
            (1u16..10).prop_map(ChildMeasurement::Flex),
            (1usize..200).prop_map(ChildMeasurement::Content),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        #[test]
        fn widths_count_matches_children_count(
            available in 10usize..500,
            children in proptest::collection::vec(arb_child_measurement(), 0..10)
        ) {
            let input = FlexLayoutInput { available, children: children.clone() };
            let result = calculate_row_widths(&input);
            prop_assert_eq!(result.widths.len(), children.len());
        }

        #[test]
        fn fixed_and_content_preserved(
            available in 10usize..500,
            children in proptest::collection::vec(arb_child_measurement(), 1..10)
        ) {
            let input = FlexLayoutInput { available, children: children.clone() };
            let result = calculate_row_widths(&input);

            for (i, child) in children.iter().enumerate() {
                match child {
                    ChildMeasurement::Fixed(w) | ChildMeasurement::Content(w) => {
                        prop_assert_eq!(result.widths[i], *w, "Fixed/Content width should be preserved");
                    }
                    ChildMeasurement::Flex(_) => {}
                }
            }
        }

        #[test]
        fn flex_children_share_remaining_space(
            fixed_width in 10usize..100,
            weights in proptest::collection::vec(1u16..5, 2..5)
        ) {
            let available = 500;
            let mut children = vec![ChildMeasurement::Fixed(fixed_width)];
            children.extend(weights.iter().map(|w| ChildMeasurement::Flex(*w)));

            let input = FlexLayoutInput { available, children: children.clone() };
            let result = calculate_row_widths(&input);

            let flex_widths: Vec<_> = result.widths[1..].to_vec();
            let total_flex: usize = flex_widths.iter().sum();
            let expected_remaining = available.saturating_sub(fixed_width);

            prop_assert!(
                total_flex <= expected_remaining,
                "Flex children shouldn't exceed remaining space: {} <= {}",
                total_flex, expected_remaining
            );
        }

        #[test]
        fn column_heights_count_matches_children(
            available in 10usize..500,
            children in proptest::collection::vec(arb_child_measurement(), 0..10),
            gap in 0usize..10
        ) {
            let heights = calculate_column_heights(available, &children, gap);
            prop_assert_eq!(heights.len(), children.len());
        }
    }

    #[test]
    fn empty_children_returns_empty() {
        let input = FlexLayoutInput {
            available: 100,
            children: vec![],
        };
        let result = calculate_row_widths(&input);
        assert!(result.widths.is_empty());
        assert_eq!(result.total_used, 0);
    }

    #[test]
    fn all_fixed_widths() {
        let input = FlexLayoutInput {
            available: 100,
            children: vec![
                ChildMeasurement::Fixed(20),
                ChildMeasurement::Fixed(30),
                ChildMeasurement::Content(15),
            ],
        };
        let result = calculate_row_widths(&input);
        assert_eq!(result.widths, vec![20, 30, 15]);
        assert_eq!(result.total_used, 65);
    }

    #[test]
    fn single_flex_takes_remaining() {
        let input = FlexLayoutInput {
            available: 100,
            children: vec![
                ChildMeasurement::Fixed(20),
                ChildMeasurement::Flex(1),
                ChildMeasurement::Fixed(10),
            ],
        };
        let result = calculate_row_widths(&input);
        assert_eq!(result.widths, vec![20, 70, 10]);
        assert_eq!(result.total_used, 100);
    }

    #[test]
    fn multiple_flex_split_proportionally() {
        let input = FlexLayoutInput {
            available: 100,
            children: vec![ChildMeasurement::Flex(1), ChildMeasurement::Flex(3)],
        };
        let result = calculate_row_widths(&input);
        assert_eq!(result.widths, vec![25, 75]);
        assert_eq!(result.total_used, 100);
    }

    #[test]
    fn flex_with_fixed_and_content() {
        let input = FlexLayoutInput {
            available: 100,
            children: vec![
                ChildMeasurement::Fixed(10),
                ChildMeasurement::Flex(1),
                ChildMeasurement::Content(20),
                ChildMeasurement::Flex(1),
            ],
        };
        let result = calculate_row_widths(&input);
        assert_eq!(result.widths[0], 10);
        assert_eq!(result.widths[2], 20);
        assert_eq!(result.widths[1], 35);
        assert_eq!(result.widths[3], 35);
        assert_eq!(result.total_used, 100);
    }

    #[test]
    fn overflow_handled_gracefully() {
        let input = FlexLayoutInput {
            available: 50,
            children: vec![ChildMeasurement::Fixed(30), ChildMeasurement::Fixed(40)],
        };
        let result = calculate_row_widths(&input);
        assert_eq!(result.widths, vec![30, 40]);
        assert_eq!(result.total_used, 70);
    }

    #[test]
    fn zero_flex_weight_gets_zero_width() {
        let input = FlexLayoutInput {
            available: 100,
            children: vec![ChildMeasurement::Flex(0), ChildMeasurement::Fixed(50)],
        };
        let result = calculate_row_widths(&input);
        assert_eq!(result.widths, vec![0, 50]);
    }

    #[test]
    fn column_heights_with_gap() {
        let children = vec![
            ChildMeasurement::Fixed(10),
            ChildMeasurement::Flex(1),
            ChildMeasurement::Fixed(10),
        ];
        let heights = calculate_column_heights(100, &children, 5);
        assert_eq!(heights[0], 10);
        assert_eq!(heights[2], 10);
        assert_eq!(heights[1], 70);
    }

    #[test]
    fn column_multiple_flex() {
        let children = vec![
            ChildMeasurement::Flex(1),
            ChildMeasurement::Flex(2),
            ChildMeasurement::Flex(1),
        ];
        let heights = calculate_column_heights(80, &children, 0);
        assert_eq!(heights, vec![20, 40, 20]);
    }

    #[test]
    fn size_to_measurement_conversion() {
        assert_eq!(
            size_to_measurement(Size::Fixed(42), 0),
            ChildMeasurement::Fixed(42)
        );
        assert_eq!(
            size_to_measurement(Size::Flex(3), 0),
            ChildMeasurement::Flex(3)
        );
        assert_eq!(
            size_to_measurement(Size::Content, 25),
            ChildMeasurement::Content(25)
        );
    }
}
