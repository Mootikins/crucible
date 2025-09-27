export interface DocumentNode {
  id: string;
  title: string;
  content: string;
  created_at: string;
  updated_at: string;
  parent_id?: string;
  children: string[];
  properties: Record<string, any>;
}

export interface CanvasNode {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  content: string;
  properties: Record<string, any>;
}

export interface CanvasEdge {
  id: string;
  from: string;
  to: string;
  label?: string;
  properties: Record<string, any>;
}

export interface ViewportState {
  zoom: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export type PropertyValue = 
  | string 
  | number 
  | boolean 
  | PropertyValue[] 
  | Record<string, PropertyValue> 
  | null;

