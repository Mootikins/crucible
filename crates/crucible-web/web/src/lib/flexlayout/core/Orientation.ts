export class Orientation {
  static HORZ = new Orientation("horz");
  static VERT = new Orientation("vert");

  static flip(from: Orientation) {
    return from === Orientation.HORZ ? Orientation.VERT : Orientation.HORZ;
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
