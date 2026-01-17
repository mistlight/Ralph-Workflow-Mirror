/**
 * Ralph Workflow - Enhanced TypeScript
 * Handles: mobile nav, install tabs, copy-to-clipboard, smooth scroll, scroll animations,
 * terminal typing effect, nav scroll detection, parallax effects, magnetic buttons,
 * cursor spotlight effect, dark mode, audience selector
 */

// Import main stylesheet
import '../styles/main.css';

// Import type definitions
import type {
  Theme,
  InstallMode,
  Audience,
  SectionMap,
  MagneticEffectConfig,
} from './types/config';
import type {
  ScrollHandler,
  ObserverCallbackHandler,
  MediaQueryChangeHandler,
} from './types/events';

(function RalphMain(): void {
  'use strict';

  // === Enhanced Magnetic Button Effect ===
  // Only apply to devices with fine pointer (mouse) for better performance on touch devices
  const hasFinePointer: boolean = window.matchMedia('(pointer: fine)').matches;
  const buttons: NodeListOf<HTMLButtonElement> = document.querySelectorAll('.btn');

  if (hasFinePointer) {
    const magneticConfig: MagneticEffectConfig = {
      moveMultiplier: 0.2,
      scaleMultiplier: 0.001,
      maxScale: 1.02,
    };

    buttons.forEach((btn: HTMLButtonElement): void => {
      btn.addEventListener('mousemove', function(this: HTMLButtonElement, e: MouseEvent): void {
        const rect: DOMRect = btn.getBoundingClientRect();
        const x: number = e.clientX - rect.left - rect.width / 2;
        const y: number = e.clientY - rect.top - rect.height / 2;

        // Enhanced magnetic effect with subtle scaling
        const moveX: number = x * magneticConfig.moveMultiplier;
        const moveY: number = y * magneticConfig.moveMultiplier;
        const distance: number = Math.sqrt(x * x + y * y);
        const scale: number = 1 + Math.min(distance * magneticConfig.scaleMultiplier, magneticConfig.maxScale - 1);

        btn.style.transform = `translate(${moveX}px, ${moveY}px) scale(${scale})`;
      });

      btn.addEventListener('mouseleave', function(this: HTMLButtonElement): void {
        btn.style.transform = '';
      });
    });

    // === Extended Magnetic Effect for Cards ===
    // Apply subtle magnetic effect to feature cards, audience cards, and open source page cards
    const magneticElements: NodeListOf<HTMLElement> = document.querySelectorAll(
      '.feature-card, .audience-card, .card, .license-card, .contribute-card, .value-card'
    );

    magneticElements.forEach((card: HTMLElement): void => {
      card.addEventListener('mousemove', function(this: HTMLElement, e: MouseEvent): void {
        const rect: DOMRect = card.getBoundingClientRect();
        const x: number = e.clientX - rect.left - rect.width / 2;
        const y: number = e.clientY - rect.top - rect.height / 2;

        // More subtle effect for cards - less movement, no scale
        const moveX: number = x * 0.03;
        const moveY: number = y * 0.03;

        card.style.transform = `translate(${moveX}px, ${moveY}px)`;
      });

      card.addEventListener('mouseleave', function(this: HTMLElement): void {
        card.style.transform = '';
      });
    });
  }

  // === Enhanced Parallax Effect for Hero Glows ===
  const heroGlows: NodeListOf<HTMLElement> = document.querySelectorAll(
    '.hero-glow, .hero-glow-2, .hero-glow-3, .hero-glow-4, .hero-glow-5'
  );

  function updateParallax(): void {
    const scrollY: number = window.scrollY;
    const windowHeight: number = window.innerHeight;

    if (scrollY < windowHeight * 1.5) {
      const parallaxValue: number = scrollY * 0.15;

      heroGlows.forEach((glow: HTMLElement, index: number): void => {
        const factor: number = (index + 1) * 0.6;
        const rotation: number = (index + 1) * 0.05;
        glow.style.transform = `translate(${parallaxValue * factor}px, ${parallaxValue * factor * 0.5}px) rotate(${rotation}deg)`;
      });
    }
  }

  // === Navigation Scroll Effect ===
  const nav: HTMLElement | null = document.querySelector('.nav');

  function updateNav(): void {
    if (!nav) return;

    const scrollY: number = window.scrollY;

    if (scrollY > 50) {
      nav.classList.add('scrolled');
    } else {
      nav.classList.remove('scrolled');
    }
  }

  // Initial call
  updateNav();

  // === Scroll-Triggered Animations (Intersection Observer) ===
  const observerOptions: IntersectionObserverInit = {
    root: null,
    rootMargin: '0px 0px -100px 0px',
    threshold: 0.1,
  };

  const scrollObserver: IntersectionObserver = new IntersectionObserver(
    ((entries: IntersectionObserverEntry[]): void => {
      entries.forEach((entry: IntersectionObserverEntry): void => {
        if (entry.isIntersecting) {
          entry.target.classList.add('is-visible');
          // Optional: unobserve after revealing for one-time animation
          // scrollObserver.unobserve(entry.target);
        }
      });
    }) as ObserverCallbackHandler,
    observerOptions
  );

  // Observe all scroll-reveal elements
  const scrollRevealElements: NodeListOf<HTMLElement> = document.querySelectorAll(
    '.scroll-reveal, .scroll-reveal-from-left, .scroll-reveal-from-right, .scroll-reveal-scale, .scroll-reveal-group'
  );

  scrollRevealElements.forEach((el: Element): void => {
    scrollObserver.observe(el);
  });

  // === Terminal Typing Effect ===
  const terminalLines: NodeListOf<HTMLElement> = document.querySelectorAll('.terminal-line');
  const terminal: HTMLElement | null = document.querySelector('.terminal-body');

  function animateTerminal(): void {
    if (!terminal) return;

    // Reset all lines
    terminalLines.forEach((line: HTMLElement): void => {
      line.classList.remove('typed');
    });

    // More realistic timing - first lines faster, slower for output
    const timing: number[] = [600, 1200, 1500, 1800, 2200, 2800, 3500];

    // Animate lines sequentially
    terminalLines.forEach((line: HTMLElement, index: number): void => {
      setTimeout((): void => {
        line.classList.add('typed');
      }, timing[index] ?? index * 600);
    });
  }

  // Run animation on page load
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', animateTerminal);
  } else {
    setTimeout(animateTerminal, 500);
  }

  // === Mobile Navigation ===
  const navToggle: HTMLButtonElement | null = document.querySelector('.nav-toggle');
  const navMenu: HTMLElement | null = document.querySelector('.nav-menu');

  if (navToggle && navMenu) {
    navToggle.addEventListener('click', function(this: HTMLButtonElement): void {
      const isOpen: boolean = navToggle.getAttribute('aria-expanded') === 'true';
      navToggle.setAttribute('aria-expanded', (!isOpen).toString());

      if (isOpen) {
        navMenu.classList.remove('mobile-open');
        document.body.style.overflow = '';
      } else {
        navMenu.classList.add('mobile-open');
        document.body.style.overflow = 'hidden';
      }
    });

    // Close menu when clicking a link
    const navLinks: NodeListOf<HTMLAnchorElement> = navMenu.querySelectorAll('.nav-link');
    navLinks.forEach((link: HTMLAnchorElement): void => {
      link.addEventListener('click', (): void => {
        navToggle.setAttribute('aria-expanded', 'false');
        navMenu.classList.remove('mobile-open');
        document.body.style.overflow = '';
      });
    });

    // Close menu when clicking outside
    document.addEventListener('click', (e: Event): void => {
      const target = e.target as Node;
      if (!navToggle.contains(target) && !navMenu.contains(target)) {
        navToggle.setAttribute('aria-expanded', 'false');
        navMenu.classList.remove('mobile-open');
        document.body.style.overflow = '';
      }
    });

    // Close menu on escape key
    document.addEventListener('keydown', (e: KeyboardEvent): void => {
      if (e.key === 'Escape' && navMenu.classList.contains('mobile-open')) {
        navToggle.setAttribute('aria-expanded', 'false');
        navMenu.classList.remove('mobile-open');
        document.body.style.overflow = '';
        navToggle.focus();
      }
    });
  }

  // === Install Tabs ===
  const installTabs: NodeListOf<HTMLButtonElement> = document.querySelectorAll('.install-tab');
  const installContents: NodeListOf<HTMLElement> = document.querySelectorAll('.install-content');

  installTabs.forEach((tab: HTMLButtonElement): void => {
    tab.addEventListener('click', function(this: HTMLButtonElement): void {
      // Validate dataset.tab exists
      const targetTab = this.dataset.tab;
      if (!targetTab) return;

      // Update active tab
      installTabs.forEach((t: HTMLButtonElement): void => t.classList.remove('install-tab-active'));
      this.classList.add('install-tab-active');

      // Show corresponding content
      installContents.forEach((content: HTMLElement): void => {
        if (content.dataset.content === targetTab) {
          content.style.display = 'block';
          content.style.opacity = '0';
          content.style.transform = 'translateY(10px)';

          // Trigger reflow for animation
          content.offsetHeight; // eslint-disable-line @typescript-eslint/no-unused-expressions

          content.style.transition = 'opacity 0.3s ease, transform 0.3s ease';
          content.style.opacity = '1';
          content.style.transform = 'translateY(0)';
        } else {
          content.style.display = 'none';
        }
      });
    });
  });

  // === Install Mode Toggle (Simple/Advanced) ===
  const installModeSwitch: HTMLElement | null = document.getElementById('install-mode-switch');
  const installSection: HTMLElement | null = document.getElementById('install');
  const simpleTabs: NodeListOf<HTMLButtonElement> = document.querySelectorAll('.install-tab-simple');
  const advancedTabs: NodeListOf<HTMLButtonElement> = document.querySelectorAll('.install-tab-advanced');
  const advancedRequirements: HTMLElement | null = document.querySelector('.install-requirements');

  // Check localStorage for saved preference with try-catch for private browsing
  let savedMode: InstallMode = 'simple';
  try {
    const stored = localStorage.getItem('ralph-install-mode');
    savedMode = (stored === 'advanced' ? 'advanced' : 'simple');
  } catch (_e) {
    // localStorage unavailable (private browsing, storage disabled)
    // Show user-visible notification that settings won't persist
    showStorageWarning();
  }

  function showStorageWarning(): void {
    // Add a small, dismissible warning banner
    const warning: HTMLDivElement = document.createElement('div');
    warning.setAttribute('role', 'alert');
    warning.style.cssText = 'position:fixed;bottom:20px;right:20px;background:#f59e0b;color:#000;padding:12px 16px;border-radius:8px;font-size:14px;font-weight:500;box-shadow:0 4px 12px rgba(0,0,0,0.15);z-index:10000;animation:slideUp 0.3s ease-out;max-width:300px;';

    const textSpan: HTMLSpanElement = document.createElement('span');
    textSpan.textContent = '⚠️ Settings won\'t save—storage disabled. ';
    warning.appendChild(textSpan);

    const closeButton: HTMLButtonElement = document.createElement('button');
    closeButton.textContent = '✕';
    closeButton.style.cssText = 'background:none;border:none;padding:0;margin-left:8px;cursor:pointer;font-weight:600;';
    closeButton.addEventListener('click', () => warning.remove());
    warning.appendChild(closeButton);

    document.body.appendChild(warning);
    setTimeout(() => warning.remove(), 8000);
  }

  if (savedMode === 'advanced') {
    installSection?.setAttribute('data-mode', 'advanced');
    installModeSwitch?.setAttribute('aria-checked', 'true');
    // Show advanced tabs
    advancedTabs.forEach((tab: HTMLButtonElement): void => {
      tab.style.display = '';
    });
    // Hide simple tabs (or keep first one)
    simpleTabs.forEach((tab: HTMLButtonElement, index: number): void => {
      if (index > 0) tab.style.display = 'none';
    });
  } else {
    installSection?.setAttribute('data-mode', 'simple');
    installModeSwitch?.setAttribute('aria-checked', 'false');
    // Hide advanced tabs
    advancedTabs.forEach((tab: HTMLButtonElement): void => {
      tab.style.display = 'none';
    });
    // Show simple tabs
    simpleTabs.forEach((tab: HTMLButtonElement): void => {
      tab.style.display = '';
    });
  }

  if (installModeSwitch) {
    installModeSwitch.addEventListener('click', function(this: HTMLElement): void {
      const isAdvanced: boolean = installModeSwitch.getAttribute('aria-checked') === 'true';
      const newMode: InstallMode = isAdvanced ? 'simple' : 'advanced';

      // Update state
      installModeSwitch.setAttribute('aria-checked', (!isAdvanced).toString());
      installSection?.setAttribute('data-mode', newMode);

      // Save preference with try-catch for private browsing
      try {
        localStorage.setItem('ralph-install-mode', newMode);
      } catch (_e) {
        // Silently fail - preference just won't persist
        console.warn('Could not save install mode preference');
      }

      // Toggle tabs visibility
      if (newMode === 'advanced') {
        advancedTabs.forEach((tab: HTMLButtonElement): void => {
          tab.style.display = '';
          tab.style.opacity = '0';
          setTimeout(() => {
            tab.style.opacity = '1';
          }, 50);
        });
        simpleTabs.forEach((tab: HTMLButtonElement, index: number): void => {
          if (index > 0) tab.style.display = 'none';
        });

        // Show full requirements in advanced mode
        if (advancedRequirements) {
          const allRequirements: NodeListOf<HTMLElement> = advancedRequirements.querySelectorAll('.requirement');
          allRequirements.forEach((req: HTMLElement): void => {
            req.style.display = '';
          });
        }
      } else {
        advancedTabs.forEach((tab: HTMLButtonElement): void => {
          tab.style.display = 'none';
        });
        simpleTabs.forEach((tab: HTMLButtonElement): void => {
          tab.style.display = '';
          tab.style.opacity = '0';
          setTimeout(() => {
            tab.style.opacity = '1';
          }, 50);
        });

        // In simple mode, only show basic requirement
        if (advancedRequirements) {
          const allRequirements: NodeListOf<HTMLElement> = advancedRequirements.querySelectorAll('.requirement');
          allRequirements.forEach((req: HTMLElement, index: number): void => {
            if (index > 0) req.style.display = 'none';
          });
        }

        // Switch back to simple tab if currently on advanced tab
        const activeTab: HTMLElement | null = document.querySelector('.install-tab-active');
        if (activeTab && activeTab.classList.contains('install-tab-advanced')) {
          const simpleTab: HTMLButtonElement | null = document.querySelector('.install-tab-simple');
          if (simpleTab) simpleTab.click();
        }
      }
    });
  }

  // === Copy to Clipboard ===
  const copyButtons: NodeListOf<HTMLButtonElement> = document.querySelectorAll('.copy-btn');

  // Helper function to create SVG checkmark element safely
  function createCheckmarkSVG(): SVGSVGElement {
    const svgNS = 'http://www.w3.org/2000/svg';
    const svg: SVGSVGElement = document.createElementNS(svgNS, 'svg') as SVGSVGElement;
    svg.setAttribute('width', '16');
    svg.setAttribute('height', '16');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '2');

    const polyline: SVGPolylineElement = document.createElementNS(svgNS, 'polyline') as SVGPolylineElement;
    polyline.setAttribute('points', '20 6 9 17 4 12');
    svg.appendChild(polyline);

    return svg;
  }

  copyButtons.forEach((btn: HTMLButtonElement): void => {
    btn.addEventListener('click', async function(this: HTMLButtonElement): Promise<void> {
      const codeBlock: HTMLElement | null = this.closest('.code-block');
      const codeElement: HTMLElement | null = codeBlock?.querySelector('code') ?? null;

      // Validate code element exists
      if (!codeElement) {
        console.warn('Copy button clicked but no code element found');
        return;
      }

      const code: string | null = codeElement.textContent;

      if (!code) {
        console.warn('Code element has no text content');
        return;
      }

      // Store original content to restore later
      const originalContent: Node = this.cloneNode(true);

      let success = false;

      // Use Clipboard API (requires secure context)
      if (navigator.clipboard && window.isSecureContext) {
        try {
          await navigator.clipboard.writeText(code);
          success = true;
        } catch (_err) {
          // Silent fail - clipboard errors are not actionable for users
        }
      }

      if (success) {
        // Show success state with animation - use DOM API
        btn.textContent = '';
        btn.appendChild(createCheckmarkSVG());
        btn.classList.add('copied');

        setTimeout((): void => {
          btn.textContent = '';
          btn.appendChild(originalContent.cloneNode(true));
          btn.classList.remove('copied');
        }, 2000);
      } else {
        // Show error indication
        btn.classList.add('copy-failed');
        setTimeout((): void => {
          btn.classList.remove('copy-failed');
        }, 2000);
      }
    });
  });

  // === Smooth Scroll ===
  const anchorLinks: NodeListOf<HTMLAnchorElement> = document.querySelectorAll('a[href^="#"]');
  anchorLinks.forEach((anchor: HTMLAnchorElement): void => {
    anchor.addEventListener('click', function(this: HTMLAnchorElement, e: MouseEvent): void {
      const targetId: string | null = this.getAttribute('href');
      if (targetId === '#' || !targetId) return;

      // Validate targetId format and extract ID value
      if (!targetId.startsWith('#') || targetId.length < 2) return;
      const idValue: string = targetId.substring(1);

      // Strict ID validation: only alphanumeric, underscore, hyphen, and must start with letter
      // This prevents injection attempts and ensures we only use valid HTML5 IDs
      if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(idValue)) return;

      // Use getElementById instead of querySelector for safer ID lookup
      // getElementById only accepts a string ID (not a selector) and is more secure
      const target: HTMLElement | null = document.getElementById(idValue);
      if (target) {
        e.preventDefault();
        const navElement: HTMLElement | null = document.querySelector('.nav');
        const navHeight: number = navElement?.offsetHeight ?? 0;
        const targetPosition: number = target.offsetTop - navHeight - 20;

        window.scrollTo({
          top: targetPosition,
          behavior: 'smooth',
        });
      }
    });
  });

  // === Active Nav Link on Scroll ===
  const sections: NodeListOf<HTMLElement> = document.querySelectorAll('section[id]');
  const navLinks: NodeListOf<HTMLAnchorElement> = document.querySelectorAll('.nav-link');

  function updateActiveNav(): void {
    let current = '';
    const scrollPos: number = window.scrollY + 150;

    sections.forEach((section: HTMLElement): void => {
      const sectionTop: number = section.offsetTop;
      const sectionHeight: number = section.offsetHeight;

      if (scrollPos >= sectionTop && scrollPos < sectionTop + sectionHeight) {
        current = section.getAttribute('id') ?? '';
      }
    });

    navLinks.forEach((link: HTMLAnchorElement): void => {
      link.classList.remove('nav-link-active');
      const href = link.getAttribute('href');
      if (href === `#${current}`) {
        link.classList.add('nav-link-active');
      }
    });
  }

  // Throttled scroll handler
  let ticking = false;
  window.addEventListener('scroll', (): void => {
    if (!ticking) {
      window.requestAnimationFrame((): void => {
        updateActiveNav();
        ticking = false;
      });
      ticking = true;
    }
  });

  // Initial call
  updateActiveNav();

  // === Enhanced Scroll Animations (Intersection Observer) ===
  const animationObserverOptions: IntersectionObserverInit = {
    root: null,
    rootMargin: '0px 0px -100px 0px',
    threshold: 0.1,
  };

  const animationObserver: IntersectionObserver = new IntersectionObserver(
    ((entries: IntersectionObserverEntry[]): void => {
      entries.forEach((entry: IntersectionObserverEntry): void => {
        if (entry.isIntersecting) {
          entry.target.classList.add('fade-in');
          (entry.target as HTMLElement).style.opacity = '1';
          (entry.target as HTMLElement).style.transform = 'translateY(0)';
          animationObserver.unobserve(entry.target);
        }
      });
    }) as ObserverCallbackHandler,
    animationObserverOptions
  );

  // Observe elements for animation
  const animatedElements: NodeListOf<HTMLElement> = document.querySelectorAll(
    '.workflow-step, .feature-card, .audience-card, .key-point, .section-header'
  );

  animatedElements.forEach((el: HTMLElement, index: number): void => {
    el.style.opacity = '0';
    el.style.transform = 'translateY(30px)';
    el.style.transition = 'opacity 0.6s ease, transform 0.6s ease';
    el.style.transitionDelay = `${index * 0.05}s`;
    animationObserver.observe(el);
  });

  // === Enhanced Scroll Animation Classes ===
  // Observe elements with new animation classes
  const enhancedAnimatedElements: NodeListOf<HTMLElement> = document.querySelectorAll(
    '.fade-in-up, .fade-in-left, .fade-in-right'
  );

  const enhancedObserver: IntersectionObserver = new IntersectionObserver(
    ((entries: IntersectionObserverEntry[]): void => {
      entries.forEach((entry: IntersectionObserverEntry): void => {
        if (entry.isIntersecting) {
          entry.target.classList.add('visible');
          enhancedObserver.unobserve(entry.target);
        }
      });
    }) as ObserverCallbackHandler,
    {
      root: null,
      rootMargin: '0px 0px -50px 0px',
      threshold: 0.15,
    }
  );

  enhancedAnimatedElements.forEach((el: Element): void => {
    enhancedObserver.observe(el);
  });

  // Cleanup function for observers (can be called if needed)
  function cleanupObservers(): void {
    animationObserver.disconnect();
    enhancedObserver.disconnect();
  }

  // Auto-cleanup on page unload to prevent memory leaks
  window.addEventListener('beforeunload', cleanupObservers);

  // === Scroll Event Listener Cleanup ===
  // Store scroll handler references for proper cleanup
  const scrollHandlers: ScrollHandler[] = [];

  function addScrollListener(handler: ScrollHandler): void {
    scrollHandlers.push(handler);
    window.addEventListener('scroll', handler);
  }

  function cleanupScrollListeners(): void {
    scrollHandlers.forEach((handler: ScrollHandler): void => {
      window.removeEventListener('scroll', handler);
    });
    scrollHandlers.length = 0;
  }

  // Register scroll listeners that need cleanup
  // Parallax scroll handler
  let parallaxTicking = false;
  const parallaxHandler = (): void => {
    if (!parallaxTicking) {
      window.requestAnimationFrame((): void => {
        updateParallax();
        parallaxTicking = false;
      });
      parallaxTicking = true;
    }
  };
  addScrollListener(parallaxHandler);

  // Nav scroll handler
  let navTicking = false;
  const navHandler = (): void => {
    if (!navTicking) {
      window.requestAnimationFrame((): void => {
        updateNav();
        navTicking = false;
      });
      navTicking = true;
    }
  };
  addScrollListener(navHandler);

  // Register cleanup on page unload
  window.addEventListener('beforeunload', cleanupScrollListeners);

  // === Character-Level Kinetic Typography ===
  // Wrap each character in hero words for individual animation
  const heroTitle: HTMLElement | null = document.querySelector('.hero-title');

  function initCharacterTypography(): void {
    if (!heroTitle) return;

    const heroWords: NodeListOf<HTMLElement> = document.querySelectorAll('.hero-word');

    heroWords.forEach((word: HTMLElement, wordIndex: number): void => {
      // Skip hero-title-accent - it uses background-clip: text for gradient effect
      // which doesn't work when text is wrapped in child spans
      if (word.classList.contains('hero-title-accent')) {
        // Just mark as complete after animation delay
        const wordText: string | null = word.textContent;
        const length = wordText?.length ?? 0;
        const totalDelay: number = (length * 0.05 + wordIndex * 0.3 + 0.3) * 1000;
        setTimeout((): void => {
          word.classList.add('word-complete');
        }, totalDelay + 600);
        return;
      }

      const text: string | null = word.textContent;
      if (!text) return;

      const charCount: number = text.length;

      // Clear existing content - use textContent for simpler operation
      while (word.firstChild) {
        word.removeChild(word.firstChild);
      }

      // Wrap each character in a span
      [...text].forEach((char: string, charIndex: number): void => {
        const span: HTMLSpanElement = document.createElement('span');
        span.textContent = char;
        span.className = 'hero-char';

        // Add character index for staggered animation
        span.style.setProperty('--char-index', charIndex.toString());
        span.style.setProperty('--word-index', wordIndex.toString());

        // Mark spaces and punctuation for special handling
        if (char === ' ') {
          span.classList.add('space');
        } else if (['.', ',', '!', '?', '&', '-'].includes(char)) {
          span.classList.add('punctuation');
        }

        word.appendChild(span);
      });

      // Mark word as complete after animation
      const totalDelay: number = (charCount * 0.05 + wordIndex * 0.3 + 0.3) * 1000;
      setTimeout((): void => {
        word.classList.add('word-complete');
      }, totalDelay + 600);
    });
  }

  // Initialize character typography on page load
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initCharacterTypography);
  } else {
    initCharacterTypography();
  }

  // === Kinetic Typography on Scroll ===
  // Note: heroTitle is already declared above in the Character Typography section
  const heroWords: NodeListOf<HTMLElement> = document.querySelectorAll('.hero-word');
  let kineticScrollY: number = window.scrollY;

  function updateKineticTypography(): void {
    const scrollY: number = window.scrollY;
    const scrollDelta: number = scrollY - kineticScrollY;
    const heroSection: HTMLElement | null = document.querySelector('.hero');

    if (!heroSection) return;

    const heroRect: DOMRect = heroSection.getBoundingClientRect();
    const heroVisible: boolean = heroRect.bottom > 0 && heroRect.top < window.innerHeight;

    if (heroVisible) {
      // Enhanced hero title transformation on scroll
      if (heroTitle) {
        const scrollProgress: number = Math.min(scrollY / (window.innerHeight * 0.5), 1);
        const scaleValue: number = 1 - scrollProgress * 0.15; // Subtle scale down
        const translateYValue: number = scrollProgress * -30; // Move up slightly

        heroTitle.style.transform = `scale(${scaleValue}) translateY(${translateYValue}px)`;

        // Add scrolling class for additional effects
        if (scrollProgress > 0.1) {
          heroTitle.classList.add('scrolling');
        } else {
          heroTitle.classList.remove('scrolling');
        }
      }

      // Individual word parallax
      heroWords.forEach((word: HTMLElement, index: number): void => {
        const speed: number = (index + 1) * 0.03; // Slightly more pronounced
        const yPos: number = scrollDelta * speed;
        const currentTransform: string = word.style.transform || 'translateY(0) scale(1)';
        const match: RegExpMatchArray | null = currentTransform.match(/translateY\(([^)]+)\)/);
        const currentY: number = match ? parseFloat(match[1]) : 0;
        const newY: number = Math.max(Math.min(currentY + yPos, 30), -30);

        word.style.transform = `translateY(${newY}px)`;
      });
    }

    kineticScrollY = scrollY;
  }

  let kineticTicking = false;
  window.addEventListener('scroll', (): void => {
    if (!kineticTicking) {
      window.requestAnimationFrame((): void => {
        updateKineticTypography();
        kineticTicking = false;
      });
      kineticTicking = true;
    }
  });

  // === Reduced Motion Support ===
  // Note: This listener is intentionally long-lived as it needs to respond to system
  // preference changes throughout the page lifecycle. No cleanup needed.
  const prefersReducedMotion: MediaQueryList = window.matchMedia('(prefers-color-scheme: light)');

  function handleReducedMotion(): void {
    if (prefersReducedMotion.matches) {
      // Disable scroll animations
      animatedElements.forEach((el: HTMLElement): void => {
        el.style.opacity = '1';
        el.style.transform = 'none';
        el.style.transition = 'none';
      });

      // Use instant scroll instead of smooth
      document.querySelectorAll('a[href^="#"]').forEach((anchor: Element): void => {
        (anchor as HTMLElement).style.scrollBehavior = 'auto';
      });

      // Disable parallax
      if (heroGlows.length > 0) {
        heroGlows.forEach((glow: HTMLElement): void => {
          glow.style.transform = '';
        });
      }
    }
  }

  handleReducedMotion();
  prefersReducedMotion.addEventListener('change', handleReducedMotion as MediaQueryChangeHandler);

  // === Cursor Spotlight Effect ===
  const cursorSpotlight: HTMLElement | null = document.querySelector('.cursor-spotlight');
  const heroSection: HTMLElement | null = document.querySelector('.hero');

  if (cursorSpotlight && heroSection && !prefersReducedMotion.matches) {
    let spotlightActive = false;

    // Activate spotlight when mouse enters hero
    heroSection.addEventListener('mouseenter', (): void => {
      spotlightActive = true;
      cursorSpotlight.classList.add('active');
    });

    heroSection.addEventListener('mouseleave', (): void => {
      spotlightActive = false;
      cursorSpotlight.classList.remove('active');
    });

    // Track mouse movement with throttling
    // Note: Event listener stays attached but only processes when spotlightActive is true
    // This is more efficient than adding/removing listeners repeatedly
    let spotlightTicking = false;
    document.addEventListener('mousemove', (e: MouseEvent): void => {
      if (!spotlightActive || !spotlightTicking) {
        requestAnimationFrame((): void => {
          if (spotlightActive && cursorSpotlight) {
            cursorSpotlight.style.left = `${e.clientX}px`;
            cursorSpotlight.style.top = `${e.clientY}px`;
          }
          spotlightTicking = false;
        });
        spotlightTicking = true;
      }
    });
  }

  // === Dark Mode Toggle ===
  const darkModeToggle: HTMLElement | null = document.getElementById('dark-mode-toggle');

  // Check for saved theme preference or system preference
  let savedTheme: Theme | null = null;
  try {
    const stored = localStorage.getItem('ralph-theme');
    savedTheme = (stored === 'light' || stored === 'dark') ? stored : null;
  } catch (_e) {
    showStorageWarning();
  }

  // Function to set theme
  function setTheme(theme: Theme): void {
    if (theme === 'light') {
      document.documentElement.setAttribute('data-theme', 'light');
    } else {
      document.documentElement.removeAttribute('data-theme');
    }
  }

  // Initialize theme
  if (savedTheme) {
    setTheme(savedTheme);
  } else if (window.matchMedia('(prefers-color-scheme: light)').matches) {
    setTheme('light');
  }
  // Default is dark (no attribute needed)

  // Dark mode toggle functionality
  if (darkModeToggle) {
    darkModeToggle.addEventListener('click', function(this: HTMLElement): void {
      const currentTheme: string | null = document.documentElement.getAttribute('data-theme');
      // null or absent means dark (default), 'light' means light mode
      const newTheme: Theme = currentTheme === 'light' ? 'dark' : 'light';

      setTheme(newTheme);

      // Save preference
      try {
        localStorage.setItem('ralph-theme', newTheme);
      } catch (_e) {
        showStorageWarning();
      }

      // Add transition animation
      darkModeToggle.style.transform = 'rotate(180deg)';
      setTimeout((): void => {
        darkModeToggle.style.transform = '';
      }, 300);
    });
  }

  // === Scroll Progress Indicator ===
  const scrollProgress: HTMLElement | null = document.getElementById('scroll-progress');

  function updateScrollProgress(): void {
    if (!scrollProgress) return;

    const windowHeight: number = document.documentElement.scrollHeight - document.documentElement.clientHeight;
    const scrolled: number = (window.scrollY / windowHeight) * 100;
    const progress: number = Math.min(Math.max(scrolled, 0), 100);

    scrollProgress.style.width = `${progress}%`;
  }

  // Throttled scroll handler
  let scrollProgressTicking = false;
  window.addEventListener('scroll', (): void => {
    if (!scrollProgressTicking) {
      window.requestAnimationFrame((): void => {
        updateScrollProgress();
        scrollProgressTicking = false;
      });
      scrollProgressTicking = true;
    }
  });

  // Initial call
  updateScrollProgress();

  // === Audience Selector ===
  const audienceOptions: NodeListOf<HTMLButtonElement> = document.querySelectorAll('.audience-option');
  const audienceSelector: HTMLElement | null = document.getElementById('audience-selector');

  // Check localStorage for saved audience preference with try-catch for private browsing
  let savedAudience: Audience | null = null;
  try {
    const stored = localStorage.getItem('ralph-audience');
    if (stored && ['developer', 'vibe-coder', 'newcomer'].includes(stored)) {
      savedAudience = stored as Audience;
    }
  } catch (_e) {
    showStorageWarning();
  }

  if (savedAudience && audienceSelector) {
    document.body.setAttribute('data-audience', savedAudience);
    audienceOptions.forEach((option: HTMLButtonElement): void => {
      if (option.dataset.audience === savedAudience) {
        option.setAttribute('aria-pressed', 'true');
      }
    });
  }

  audienceOptions.forEach((option: HTMLButtonElement): void => {
    option.addEventListener('click', function(this: HTMLButtonElement): void {
      // Validate dataset.audience exists and is a valid value
      const audience = this.dataset.audience;
      const validAudiences: Audience[] = ['developer', 'vibe-coder', 'newcomer'];
      if (!audience || !validAudiences.includes(audience as Audience)) return;

      const typedAudience = audience as Audience;

      // Update button states
      audienceOptions.forEach((opt: HTMLButtonElement): void => {
        opt.setAttribute('aria-pressed', 'false');
      });
      this.setAttribute('aria-pressed', 'true');

      // Update body attribute for content filtering
      if (document.body.getAttribute('data-audience') === typedAudience) {
        // Toggle off if clicking the same option
        document.body.removeAttribute('data-audience');
        try {
          localStorage.removeItem('ralph-audience');
        } catch (_e) {
          showStorageWarning();
        }
      } else {
        document.body.setAttribute('data-audience', typedAudience);
        try {
          localStorage.setItem('ralph-audience', typedAudience);
        } catch (_e) {
          showStorageWarning();
        }
      }

      // Smooth scroll to relevant section based on audience
      const sectionMap: SectionMap = {
        developer: '#features',
        'vibe-coder': '#how-it-works',
        newcomer: '#install',
      };

      // Validate audience is a valid key before accessing sectionMap (already validated above)
      if (!Object.prototype.hasOwnProperty.call(sectionMap, typedAudience)) return;

      const targetSection: string = sectionMap[typedAudience];
      if (targetSection && document.body.getAttribute('data-audience')) {
        setTimeout((): void => {
          const target: HTMLElement | null = document.querySelector(targetSection);
          if (target) {
            const navElement: HTMLElement | null = document.querySelector('.nav');
            const navHeight: number = navElement?.offsetHeight ?? 0;
            const targetPosition: number = target.offsetTop - navHeight - 20;

            window.scrollTo({
              top: targetPosition,
              behavior: 'smooth',
            });
          }
        }, 300);
      }
    });
  });

  // === Platform Detection for Install Tabs (Optional Enhancement) ===
  function detectPlatform(): void {
    // All users use the same install method now (git clone)
    const defaultTab = 'install';

    // Click the default tab
    const defaultTabButton: HTMLElement | null = document.querySelector(`[data-tab="${defaultTab}"]`);
    if (defaultTabButton && !document.querySelector('.install-tab-active')) {
      (defaultTabButton as HTMLButtonElement).click();
    }
  }

  // Only auto-select on first visit - with try-catch for private browsing
  let tabSelected = false;
  try {
    tabSelected = localStorage.getItem('ralph-tab-selected') === 'true';
  } catch (_e) {
    showStorageWarning();
  }

  if (!tabSelected) {
    detectPlatform();
    try {
      localStorage.setItem('ralph-tab-selected', 'true');
    } catch (_e) {
      showStorageWarning();
    }
  }

  // === Expandable Feature Cards ===
  const expandButtons: NodeListOf<HTMLButtonElement> = document.querySelectorAll('.feature-expand-btn');

  expandButtons.forEach((btn: HTMLButtonElement): void => {
    btn.addEventListener('click', function(this: HTMLButtonElement): void {
      // Validate dataset.expand exists
      const targetId = btn.dataset.expand;
      if (!targetId) return;
      const targetContent: HTMLElement | null = document.getElementById(targetId);
      const isExpanded: boolean = btn.getAttribute('aria-expanded') === 'true';

      // Validate targetContent exists before manipulation
      if (!targetContent) {
        console.warn(`Expand button references missing element: ${targetId}`);
        return;
      }

      // Toggle expanded state
      btn.setAttribute('aria-expanded', (!isExpanded).toString());
      targetContent.setAttribute('aria-hidden', isExpanded.toString());

      // Update button text
      const span: HTMLSpanElement | null = btn.querySelector('span');
      if (span) {
        span.textContent = isExpanded ? 'Learn more' : 'Show less';
      }
    });
  });
})();
