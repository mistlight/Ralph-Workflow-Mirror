/**
 * Configuration Interfaces
 * Type definitions for various configuration objects used throughout the application
 */

/**
 * Intersection Observer options
 */
export interface ObserverOptions {
  root: Element | null;
  rootMargin: string;
  threshold: number | number[];
}

/**
 * Animation timing configuration
 */
export interface AnimationTiming {
  delay: number;
  duration: number;
  easing: string;
}

/**
 * Magnetic effect configuration
 */
export interface MagneticEffectConfig {
  moveMultiplier: number;
  scaleMultiplier: number;
  maxScale: number;
}

/**
 * Parallax effect configuration
 */
export interface ParallaxConfig {
  speed: number;
  rotation: number;
  factor: number;
}

/**
 * Theme configuration
 */
export type Theme = 'light' | 'dark';

/**
 * Install mode configuration
 */
export type InstallMode = 'simple' | 'advanced';

/**
 * Audience type
 */
export type Audience = 'developer' | 'vibe-coder' | 'newcomer';

/**
 * Section to audience mapping
 */
export interface SectionMap {
  readonly [key: string]: string;
  readonly developer: '#features';
  readonly 'vibe-coder': '#how-it-works';
  readonly newcomer: '#install';
}

/**
 * Install tab data attribute
 */
export interface InstallTabData extends DOMStringMap {
  tab?: string;
}

/**
 * Install content data attribute
 */
export interface InstallContentData extends DOMStringMap {
  content?: string;
}

/**
 * Audience option data attribute
 */
export interface AudienceOptionData extends DOMStringMap {
  audience?: Audience;
}

/**
 * Expand button data attribute
 */
export interface ExpandButtonData extends DOMStringMap {
  expand?: string;
}
