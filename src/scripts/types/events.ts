/**
 * Event Handler Types
 * Type definitions for custom event handlers and options
 */

/**
 * Mouse move event handler with position data
 */
export type MouseMoveHandler = (event: MouseEvent) => void;

/**
 * Scroll event handler with throttling support
 */
export type ThrottledScrollHandler = () => void;

/**
 * Click event handler for elements with dataset
 */
export type ClickHandlerWithData<T extends HTMLElement> = (
  this: T,
  event: MouseEvent
) => void;

/**
 * Keyboard event handler
 */
export type KeyboardEventHandler = (event: KeyboardEvent) => void;

/**
 * Intersection observer callback handler
 */
export type ObserverCallbackHandler = (entries: IntersectionObserverEntry[]) => void;

/**
 * Media query change handler
 */
export type MediaQueryChangeHandler = (event: MediaQueryListEvent) => void;

/**
 * Storage event handler
 */
export type StorageHandler = (event: StorageEvent) => void;

/**
 * Event listener cleanup function
 */
export type CleanupFunction = () => void;

/**
 * Event listener options
 */
export interface EventListenerOptions {
  capture?: boolean;
  once?: boolean;
  passive?: boolean;
  signal?: AbortSignal;
}

/**
 * Scroll handler function type
 */
export type ScrollHandler = () => void;

/**
 * Scroll event manager for handling multiple scroll listeners
 */
export interface ScrollEventManager {
  handlers: ScrollHandler[];
  add: (handler: ScrollHandler) => void;
  cleanup: () => void;
}

/**
 * Mouse event with element position data
 */
export interface MousePositionEvent extends MouseEvent {
  readonly elementX: number;
  readonly elementY: number;
  readonly centerX: number;
  readonly centerY: number;
}
