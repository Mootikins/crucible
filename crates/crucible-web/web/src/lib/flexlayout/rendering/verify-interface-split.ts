/**
 * Verification script: confirms all ILayoutContext members are assigned
 * to either ICoreRenderContext or IBindingContext with no gaps or duplicates.
 *
 * Run: bun run src/lib/flexlayout/rendering/verify-interface-split.ts
 */

const layoutContextMembers = [
    "model",
    "factory",
    "getClassName",
    "doAction",
    "customizeTab",
    "customizeTabSet",
    "getRootDiv",
    "getMainElement",
    "getDomRect",
    "getBoundingClientRect",
    "getWindowId",
    "setEditingTab",
    "getEditingTab",
    "isRealtimeResize",
    "getLayoutRootDiv",
    "onFloatDragStart",
    "onFloatDock",
    "onFloatClose",
    "redraw",
    "setDragNode",
    "clearDragMain",
    "getRevision",
    "showPopup",
    "showContextMenu",
] as const;

const coreMembers = [
    "model",
    "doAction",
    "getClassName",
    "getRootDiv",
    "getMainElement",
    "getLayoutRootDiv",
    "getDomRect",
    "getBoundingClientRect",
    "getWindowId",
    "isRealtimeResize",
    "redraw",
    "getRevision",
    "setDragNode",
    "clearDragMain",
    "setEditingTab",
    "getEditingTab",
] as const;

const bindingMembers = [
    "factory",
    "customizeTab",
    "customizeTabSet",
    "showPopup",
    "showContextMenu",
    "onFloatDragStart",
    "onFloatDock",
    "onFloatClose",
] as const;

const allAssigned = [...coreMembers, ...bindingMembers];
const missing = layoutContextMembers.filter(m => !allAssigned.includes(m as any));
const duplicates = allAssigned.filter((m, i) => allAssigned.indexOf(m) !== i);

console.log(`ILayoutContext members: ${layoutContextMembers.length}`);
console.log(`ICoreRenderContext:     ${coreMembers.length}`);
console.log(`IBindingContext:        ${bindingMembers.length}`);
console.log(`Total assigned:         ${allAssigned.length} / ${layoutContextMembers.length}`);
console.log();

if (missing.length > 0) {
    console.error("MISSING members:", missing);
    process.exit(1);
}
if (duplicates.length > 0) {
    console.error("DUPLICATE members:", duplicates);
    process.exit(1);
}
if (allAssigned.length !== layoutContextMembers.length) {
    console.error(`COUNT MISMATCH: ${allAssigned.length} assigned vs ${layoutContextMembers.length} total`);
    process.exit(1);
}

console.log("All members accounted for. No gaps, no duplicates.");
