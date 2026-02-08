/**
 * JSON model interfaces for FlexLayout serialization/deserialization
 */

export interface IJsonModel {
  global?: IGlobalAttributes;
  borders?: IJsonBorderNode[];
  layout?: IJsonRowNode | IJsonTabSetNode;
  windows?: Record<string, any>;
  floatZOrder?: string[];
}

export interface IGlobalAttributes {
  [key: string]: unknown;
}

export interface IJsonNode {
  type: string;
  id?: string;
  [key: string]: unknown;
}

export interface IJsonRowNode extends IJsonNode {
  type: "row";
  weight?: number;
  children?: (IJsonRowNode | IJsonTabSetNode | IJsonTabNode)[];
}

export interface IJsonTabSetNode extends IJsonNode {
  type: "tabset";
  weight?: number;
  name?: string;
  children?: IJsonTabNode[];
  [key: string]: unknown;
}

export interface IJsonTabNode extends IJsonNode {
  type: "tab";
  name?: string;
  component?: string;
  icon?: string;
  config?: unknown;
  [key: string]: unknown;
}

export interface IJsonBorderNode extends IJsonNode {
	type: "border";
	location?: string;
	children?: IJsonTabNode[];
}

export interface IJsonPopout extends IJsonNode {
	type: "popout";
	window?: Window;
	[key: string]: unknown;
}

export interface AnchoredBox {
	width: number;
	height: number;
	top?: number;
	bottom?: number;
	left?: number;
	right?: number;
}
