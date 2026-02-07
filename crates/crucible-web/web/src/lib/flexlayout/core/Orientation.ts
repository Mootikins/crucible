export class Orientation {
  static HORZ = new Orientation("horz");
  static VERT = new Orientation("vert");

  static flip(from: Orientation) {
    if (from === Orientation.HORZ) {
      return Orientation.VERT;
    } else {
      return Orientation.HORZ;
    }
  }

  private _name: string;

  private constructor(name: string) {
    this._name = name;
  }

  getName() {
    return this._name;
  }

  toString() {
    return this._name;
  }
}
