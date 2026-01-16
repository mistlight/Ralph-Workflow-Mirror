/**
 * DOM Element Type Guards
 * Provides type-safe query selectors and element type checking
 */

/**
 * Type guard to check if an element is an HTMLButtonElement
 */
export function isButtonElement(element: HTMLElement): element is HTMLButtonElement {
  return element.tagName === 'BUTTON';
}

/**
 * Type guard to check if an element is an HTMLAnchorElement
 */
export function isAnchorElement(element: HTMLElement): element is HTMLAnchorElement {
  return element.tagName === 'A';
}

/**
 * Type guard to check if an element is an HTMLInputElement
 */
export function isInputElement(element: HTMLElement): element is HTMLInputElement {
  return element.tagName === 'INPUT';
}

/**
 * Type-safe querySelector that returns null if element not found
 */
export function querySelector<E extends Element = Element>(
  parent: ParentNode,
  selectors: string
): E | null {
  return parent.querySelector<E>(selectors);
}

/**
 * Type-safe querySelectorAll that returns NodeList of specified type
 */
export function querySelectorAll<E extends Element = Element>(
  parent: ParentNode,
  selectors: string
): NodeListOf<E> {
  return parent.querySelectorAll<E>(selectors);
}

/**
 * Type-safe getElementById that returns null if element not found
 */
export function getElementById<E extends Element = HTMLElement>(
  id: string
): E | null {
  return document.getElementById(id) as E | null;
}

/**
 * Require an element to exist, throw error if not found
 * Use when element is required for functionality
 */
export function requireElement<E extends Element = HTMLElement>(
  selector: string,
  parent: ParentNode = document
): E {
  const element = parent.querySelector<E>(selector);
  if (!element) {
    throw new Error(`Required element not found: ${selector}`);
  }
  return element;
}

/**
 * Require element by ID to exist, throw error if not found
 */
export function requireElementById<E extends Element = HTMLElement>(id: string): E {
  const element = document.getElementById(id) as E | null;
  if (!element) {
    throw new Error(`Required element with id not found: ${id}`);
  }
  return element;
}

/**
 * Get closest parent element matching selector
 */
export function getClosestElement<E extends Element = Element>(
  element: Element,
  selector: string
): E | null {
  return element.closest<E>(selector);
}
