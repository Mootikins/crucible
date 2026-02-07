/**
 * FlexLayout Upstream Helper Functions Reference
 * 
 * This file contains ALL helper functions extracted from the upstream FlexLayout
 * repository (caplin/FlexLayout) tests-playwright/helpers.ts
 * 
 * These are used as a reference for exact test helper reproduction in Task 11.
 * All helper functions must be ported with matching signatures and behavior.
 */

// Helper 1: findAllTabSets
// Signature: (page: Page) => Locator
// Purpose: Find all tabset elements on the page
// Implementation: page.locator('.flexlayout__tabset')

// Helper 2: findPath
// Signature: (page: Page, path: string) => Locator
// Purpose: Find element by data-layout-path attribute
// Implementation: page.locator(`[data-layout-path="${path}"]`)

// Helper 3: findTabButton
// Signature: (page: Page, path: string, index: number) => Locator
// Purpose: Find tab button at specific index within a path
// Implementation: findPath(page, `${path}/tb${index}`)

// Helper 4: checkTab
// Signature: async (page: Page, path: string, index: number, selected: boolean, text: string) => Promise<void>
// Purpose: Verify tab button and content visibility, selection state, and text
// Assertions:
//   - tabButton.toBeVisible()
//   - tabButton.toHaveClass(selected ? 'flexlayout__tab_button--selected' : 'flexlayout__tab_button--unselected')
//   - tabButton.locator('.flexlayout__tab_button_content').toContainText(text)
//   - tabContent.toBeVisible({ visible: selected })
//   - tabContent.toContainText(text)

// Helper 5: checkBorderTab
// Signature: async (page: Page, path: string, index: number, selected: boolean, text: string) => Promise<void>
// Purpose: Verify border tab button and content visibility, selection state, and text
// Assertions:
//   - tabButton.toBeVisible()
//   - tabButton.toHaveClass(selected ? 'flexlayout__border_button--selected' : 'flexlayout__border_button--unselected')
//   - tabButton.locator('.flexlayout__border_button_content').toContainText(text)
//   - if selected: tabContent.toBeVisible() && tabContent.toContainText(text)

// Helper 6: Location enum
// Values: CENTER, TOP, BOTTOM, LEFT, RIGHT, LEFTEDGE
// Purpose: Specify drop location for drag operations

// Helper 7: getLocation
// Signature: (rect: { x: number; y: number; width: number; height: number }, loc: Location) => { x: number; y: number }
// Purpose: Calculate coordinates for a specific location within a bounding box
// Locations:
//   - CENTER: (x + width/2, y + height/2)
//   - TOP: (x + width/2, y + 5)
//   - BOTTOM: (x + width/2, y + height - 5)
//   - LEFT: (x + 5, y + height/2)
//   - RIGHT: (x + width - 5, y + height/2)
//   - LEFTEDGE: (x, y + height/2)

// Helper 8: drag
// Signature: async (page: Page, from: Locator, to: Locator, loc: Location) => Promise<void>
// Purpose: Drag element from one location to another with specified drop location
// Steps:
//   1. Get bounding boxes for both elements
//   2. Calculate center of source element
//   3. Calculate target location using getLocation()
//   4. Move mouse to source center
//   5. Press mouse down
//   6. Move mouse to target location (10 steps)
//   7. Release mouse

// Helper 9: dragToEdge
// Signature: async (page: Page, from: Locator, edgeIndex: number) => Promise<void>
// Purpose: Drag element to a specific edge drop zone
// Steps:
//   1. Get bounding box for source element
//   2. Calculate center of source
//   3. Move mouse to source center
//   4. Press mouse down
//   5. Move slightly to trigger edge display
//   6. Find edge element at edgeIndex from '.flexlayout__edge_rect'
//   7. Calculate center of edge
//   8. Move mouse to edge center (10 steps)
//   9. Release mouse

// Helper 10: dragSplitter
// Signature: async (page: Page, from: Locator, upDown: boolean, distance: number) => Promise<void>
// Purpose: Drag splitter horizontally or vertically by specified distance
// Parameters:
//   - upDown: true for vertical (Y-axis), false for horizontal (X-axis)
//   - distance: pixels to move (positive or negative)
// Steps:
//   1. Get bounding box for splitter
//   2. Calculate center of splitter
//   3. Calculate target position based on upDown and distance
//   4. Move mouse to splitter center
//   5. Press mouse down
//   6. Move mouse to target position (10 steps)
//   7. Release mouse

export const HELPERS_COUNT = 10;
