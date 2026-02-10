import { Orientation } from "./Orientation";
import { Rect } from "./Rect";

export class DockLocation {
  static values = new Map<string, DockLocation>();
  static TOP = new DockLocation("top", Orientation.VERT, 0);
  static BOTTOM = new DockLocation("bottom", Orientation.VERT, 1);
  static LEFT = new DockLocation("left", Orientation.HORZ, 0);
  static RIGHT = new DockLocation("right", Orientation.HORZ, 1);
  static CENTER = new DockLocation("center", Orientation.VERT, 0);

  static getByName(name: string): DockLocation {
    return DockLocation.values.get(name)!;
  }

  static getLocation(rect: Rect, x: number, y: number) {
    x = (x - rect.x) / rect.width;
    y = (y - rect.y) / rect.height;

    if (x >= 0.25 && x < 0.75 && y >= 0.25 && y < 0.75) {
      return DockLocation.CENTER;
    }

    const bl = y >= x;
    const br = y >= 1 - x;

    if (bl) {
      return br ? DockLocation.BOTTOM : DockLocation.LEFT;
    } else {
      return br ? DockLocation.RIGHT : DockLocation.TOP;
    }
  }

  name: string;
  orientation: Orientation;
  indexPlus: number;

  constructor(_name: string, _orientation: Orientation, _indexPlus: number) {
    this.name = _name;
    this.orientation = _orientation;
    this.indexPlus = _indexPlus;
    DockLocation.values.set(this.name, this);
  }

  getName() {
    return this.name;
  }

  getOrientation() {
    return this.orientation;
  }

  getDockRect(r: Rect) {
    switch (this) {
      case DockLocation.TOP:
        return new Rect(r.x, r.y, r.width, r.height / 2);
      case DockLocation.BOTTOM:
        return new Rect(r.x, r.getBottom() - r.height / 2, r.width, r.height / 2);
      case DockLocation.LEFT:
        return new Rect(r.x, r.y, r.width / 2, r.height);
      case DockLocation.RIGHT:
        return new Rect(r.getRight() - r.width / 2, r.y, r.width / 2, r.height);
      default:
        return r.clone();
    }
  }

  split(rect: Rect, size: number) {
    switch (this) {
      case DockLocation.TOP: {
        const r1 = new Rect(rect.x, rect.y, rect.width, size);
        const r2 = new Rect(rect.x, rect.y + size, rect.width, rect.height - size);
        return { start: r1, end: r2 };
      }
      case DockLocation.LEFT: {
        const r1 = new Rect(rect.x, rect.y, size, rect.height);
        const r2 = new Rect(rect.x + size, rect.y, rect.width - size, rect.height);
        return { start: r1, end: r2 };
      }
      case DockLocation.RIGHT: {
        const r1 = new Rect(rect.getRight() - size, rect.y, size, rect.height);
        const r2 = new Rect(rect.x, rect.y, rect.width - size, rect.height);
        return { start: r1, end: r2 };
      }
      default: {
        // BOTTOM
        const r1 = new Rect(rect.x, rect.getBottom() - size, rect.width, size);
        const r2 = new Rect(rect.x, rect.y, rect.width, rect.height - size);
        return { start: r1, end: r2 };
      }
    }
  }

  reflect() {
    switch (this) {
      case DockLocation.TOP:
        return DockLocation.BOTTOM;
      case DockLocation.BOTTOM:
        return DockLocation.TOP;
      case DockLocation.LEFT:
        return DockLocation.RIGHT;
      case DockLocation.RIGHT:
        return DockLocation.LEFT;
      default:
        return DockLocation.TOP;
    }
  }

  toString() {
    return "(DockLocation: name=" + this.name + ", orientation=" + this.orientation + ")";
  }
}
