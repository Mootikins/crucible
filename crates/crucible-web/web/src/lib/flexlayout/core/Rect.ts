import { Orientation } from "./Orientation";

export interface IJsonRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export class Rect {
  static empty() {
    return new Rect(0, 0, 0, 0);
  }

  static fromJson(json: IJsonRect): Rect {
    return new Rect(json.x, json.y, json.width, json.height);
  }

  x: number;
  y: number;
  width: number;
  height: number;

  constructor(x: number, y: number, width: number, height: number) {
    this.x = x;
    this.y = y;
    this.width = width;
    this.height = height;
  }

  toJson() {
    return { x: this.x, y: this.y, width: this.width, height: this.height };
  }

  snap(round: number) {
    this.x = Math.round(this.x / round) * round;
    this.y = Math.round(this.y / round) * round;
    this.width = Math.round(this.width / round) * round;
    this.height = Math.round(this.height / round) * round;
  }

  relativeTo(r: Rect | DOMRect) {
    return new Rect(this.x - r.x, this.y - r.y, this.width, this.height);
  }

  clone() {
    return new Rect(this.x, this.y, this.width, this.height);
  }

  equals(rect: Rect | null | undefined) {
    return (
      this.x === rect?.x &&
      this.y === rect?.y &&
      this.width === rect?.width &&
      this.height === rect?.height
    );
  }

  equalSize(rect: Rect | null | undefined) {
    return this.width === rect?.width && this.height === rect?.height;
  }

  getBottom() {
    return this.y + this.height;
  }

  getRight() {
    return this.x + this.width;
  }

  get bottom() {
    return this.y + this.height;
  }

  get right() {
    return this.x + this.width;
  }

  getCenter() {
    return { x: this.x + this.width / 2, y: this.y + this.height / 2 };
  }

  contains(x: number, y: number) {
    if (this.x <= x && x <= this.getRight() && this.y <= y && y <= this.getBottom()) {
      return true;
    } else {
      return false;
    }
  }

  removeInsets(insets: { top: number; left: number; bottom: number; right: number }) {
    return new Rect(
      this.x + insets.left,
      this.y + insets.top,
      Math.max(0, this.width - insets.left - insets.right),
      Math.max(0, this.height - insets.top - insets.bottom)
    );
  }

  centerInRect(outerRect: Rect) {
    this.x = (outerRect.width - this.width) / 2;
    this.y = (outerRect.height - this.height) / 2;
  }

  _getSize(orientation: Orientation) {
    let prefSize = this.width;
    if (orientation === Orientation.VERT) {
      prefSize = this.height;
    }
    return prefSize;
  }

	positionElement(element: HTMLElement, position?: string) {
		this.styleWithPosition(element.style, position);
	}

	styleWithPosition(style?: Record<string, any>, position: string = "absolute") {
		if (style == null) {
			style = {};
		}
		style.left = this.x + "px";
		style.top = this.y + "px";
		style.width = Math.max(0, this.width) + "px";
		style.height = Math.max(0, this.height) + "px";
		style.position = position;
		return style;
	}

	toString() {
		return (
			"(Rect: x=" +
			this.x +
			", y=" +
			this.y +
			", width=" +
			this.width +
			", height=" +
			this.height +
			")"
		);
	}
}
