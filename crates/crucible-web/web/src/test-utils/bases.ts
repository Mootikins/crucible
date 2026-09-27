import type { components } from '@/lib/api-schema';

type ViewOptions = components['schemas']['ViewOptions'];

/**
 * The view options the daemon sends for a view that sets none
 * (`crucible-daemon/src/bases/view_options.rs`), with the overrides of one test.
 */
export function baseOptions(overrides: Partial<ViewOptions> = {}): ViewOptions {
  return {
    card_size: 200,
    column_width: 280,
    image: null,
    image_fit: 'cover',
    image_aspect_ratio: 1,
    hide_empty_groups: false,
    markers: 'bullet',
    indent_properties: false,
    separator: ', ',
    row_height: 'short',
    column_size: {},
    ...overrides,
  };
}
