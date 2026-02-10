/**
 * Event system for Model change notifications
 * Based on Dockview's Event<T> pattern
 */

export interface IDisposable {
  dispose(): void;
}

/**
 * Event<T> is a function that registers a listener and returns a disposable
 * to unregister it. This pattern allows for type-safe event handling without
 * a heavyweight EventEmitter class.
 */
export type Event<T> = (listener: (e: T) => any) => IDisposable;

/**
 * Emitter<T> manages a list of listeners and provides an Event<T> property
 * for registration. Call fire() to notify all listeners.
 */
export class Emitter<T> {
  private listeners: Array<(e: T) => any> = [];

  /**
   * The event property that consumers use to register listeners
   */
  get event(): Event<T> {
    return (listener: (e: T) => any) => {
      this.listeners.push(listener);
      return {
        dispose: () => {
          const index = this.listeners.indexOf(listener);
          if (index >= 0) {
            this.listeners.splice(index, 1);
          }
        }
      };
    };
  }

  /**
   * Fire the event, notifying all registered listeners
   */
  fire(event: T): void {
    this.listeners.forEach(listener => listener(event));
  }

  /**
   * Remove all listeners
   */
  dispose(): void {
    this.listeners = [];
  }
}
