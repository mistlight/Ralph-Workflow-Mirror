/**
 * Animation State Types
 * Type definitions for animation states and configurations
 */

/**
 * Scroll animation state
 */
export interface ScrollAnimationState {
  ticking: boolean;
  lastScrollY: number;
  isVisible: boolean;
}

/**
 * Kinetic typography state
 */
export interface KineticTypographyState {
  scrollY: number;
  scrollDelta: number;
  isActive: boolean;
}

/**
 * Cursor spotlight state
 */
export interface CursorSpotlightState {
  active: boolean;
  x: number;
  y: number;
  ticking: boolean;
}

/**
 * Terminal animation state
 */
export interface TerminalAnimationState {
  currentLine: number;
  isAnimating: boolean;
  timing: number[];
}

/**
 * Fade animation configuration
 */
export interface FadeAnimationConfig {
  fromOpacity: number;
  toOpacity: number;
  fromY: number;
  toY: number;
  duration: number;
  delay: number;
}

/**
 * Magnetic button state
 */
export interface MagneticButtonState {
  x: number;
  y: number;
  moveX: number;
  moveY: number;
  scale: number;
}

/**
 * Parallax element state
 */
export interface ParallaxElementState {
  currentY: number;
  targetY: number;
  speed: number;
}
