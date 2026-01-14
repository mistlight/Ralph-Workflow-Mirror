/**
 * Ralph Workflow - Premium Interactive Scripts
 * Enhanced with magnetic effects, smooth animations, and refined interactions
 */

// === UTILITIES ===
const Utils = {
    // Throttle function for performance
    throttle(func, wait) {
        let timeout;
        let previous = 0;
        return function(...args) {
            const now = Date.now();
            const remaining = wait - (now - previous);
            if (remaining <= 0 || remaining > wait) {
                if (timeout) {
                    clearTimeout(timeout);
                    timeout = null;
                }
                previous = now;
                func.apply(this, args);
            } else if (!timeout) {
                timeout = setTimeout(() => {
                    previous = Date.now();
                    timeout = null;
                    func.apply(this, args);
                }, remaining);
            }
        };
    },

    // Check for reduced motion preference
    prefersReducedMotion() {
        return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    },

    // Smooth easing function
    easeOutCubic(t) {
        return 1 - Math.pow(1 - t, 3);
    }
};

// === THEME TOGGLE ===
const ThemeToggle = {
    STORAGE_KEY: 'ralph-theme',

    init() {
        this.toggle = document.querySelector('.theme-toggle');
        if (!this.toggle) return;

        // Load saved theme or prefer dark mode
        const savedTheme = localStorage.getItem(this.STORAGE_KEY);
        const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        const theme = savedTheme || (prefersDark ? 'dark' : 'light');

        this.setTheme(theme);
        this.toggle.addEventListener('click', () => this.toggleTheme());
    },

    setTheme(theme) {
        document.body.setAttribute('data-theme', theme);
        const icon = this.toggle?.querySelector('.theme-icon');
        if (icon) icon.setAttribute('data-theme', theme);
        localStorage.setItem(this.STORAGE_KEY, theme);

        // Add subtle transition to body
        document.body.style.transition = 'background-color 0.3s ease, color 0.3s ease';
        setTimeout(() => {
            document.body.style.transition = '';
        }, 300);
    },

    toggleTheme() {
        const current = document.body.getAttribute('data-theme');
        const next = current === 'dark' ? 'light' : 'dark';
        this.setTheme(next);
    }
};

// === MOBILE NAVIGATION ===
const MobileNav = {
    init() {
        this.toggle = document.querySelector('.nav-toggle');
        this.menu = document.querySelector('.nav-menu');
        if (!this.toggle || !this.menu) return;

        this.toggle.addEventListener('click', () => this.toggleMenu());

        // Close menu on link click
        this.menu.querySelectorAll('.nav-link').forEach(link => {
            link.addEventListener('click', () => this.closeMenu());
        });

        // Close on escape
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') this.closeMenu();
        });

        // Close on outside click
        document.addEventListener('click', (e) => {
            if (!e.target.closest('.nav')) this.closeMenu();
        });
    },

    toggleMenu() {
        const isOpen = this.menu.classList.toggle('nav-menu--active');
        this.toggle.setAttribute('aria-expanded', isOpen);
        document.body.style.overflow = isOpen ? 'hidden' : '';

        // Animate hamburger
        if (isOpen) {
            this.toggle.classList.add('nav-toggle--active');
        } else {
            this.toggle.classList.remove('nav-toggle--active');
        }
    },

    closeMenu() {
        this.menu.classList.remove('nav-menu--active');
        this.toggle.setAttribute('aria-expanded', 'false');
        document.body.style.overflow = '';
        this.toggle.classList.remove('nav-toggle--active');
    }
};

// === SMOOTH SCROLL ===
const SmoothScroll = {
    init() {
        document.querySelectorAll('a[href^="#"]').forEach(anchor => {
            anchor.addEventListener('click', (e) => {
                const href = anchor.getAttribute('href');
                if (href === '#') return;

                const target = document.querySelector(href);
                if (!target) return;

                e.preventDefault();
                const navHeight = document.querySelector('.nav')?.offsetHeight || 0;
                const targetPosition = target.offsetTop - navHeight;

                window.scrollTo({
                    top: targetPosition,
                    behavior: 'smooth'
                });
            });
        });
    }
};

// === SCROLL ANIMATIONS ===
const ScrollAnimations = {
    init() {
        if (Utils.prefersReducedMotion()) return;

        const observerOptions = {
            threshold: 0.15,
            rootMargin: '0px 0px -80px 0px'
        };

        this.observer = new IntersectionObserver((entries) => {
            entries.forEach(entry => {
                if (entry.isIntersecting) {
                    entry.target.classList.add('animate-in');
                    // Optional: unobserve after animation for one-time effect
                    // this.observer.unobserve(entry.target);
                }
            });
        }, observerOptions);

        this.observeElements();
    },

    observeElements() {
        const selectors = [
            '.value-card',
            '.preset-card',
            '.feature-card',
            '.audience-card',
            '.workflow-step',
            '.use-case',
            '.flow-step'
        ];

        selectors.forEach(selector => {
            document.querySelectorAll(selector).forEach((el, index) => {
                el.style.opacity = '0';
                el.style.transform = 'translateY(30px)';
                el.style.transition = 'opacity 0.6s cubic-bezier(0.16, 1, 0.3, 1), transform 0.6s cubic-bezier(0.16, 1, 0.3, 1)';
                // Stagger delay based on index
                const delay = (index % 6) * 0.08;
                el.style.transitionDelay = `${delay}s`;
                this.observer.observe(el);
            });
        });
    }
};

// Add animate-in styles dynamically
const animationStyles = document.createElement('style');
animationStyles.textContent = `
    .animate-in {
        opacity: 1 !important;
        transform: translateY(0) !important;
    }
`;
document.head.appendChild(animationStyles);

// === NAVIGATION SCROLL EFFECT ===
const NavScroll = {
    init() {
        this.nav = document.querySelector('.nav');
        if (!this.nav) return;

        let lastScroll = 0;
        const scrollHandler = Utils.throttle(() => {
            const currentScroll = window.scrollY;

            // Add shadow when scrolled
            if (currentScroll > 50) {
                this.nav.classList.add('scrolled');
            } else {
                this.nav.classList.remove('scrolled');
            }

            lastScroll = currentScroll;
        }, 100);

        window.addEventListener('scroll', scrollHandler, { passive: true });
    }
};

// === TERMINAL TYPING EFFECT ===
const TerminalTyping = {
    commands: [
        'ralph -S',
        'ralph --agent claude-code',
        'ralph -Q --diagnose'
    ],
    outputs: [
        '[INFO] Starting Ralph workflow...',
        '[INFO] Loading PROMPT.md',
        '[INFO] Developer agent: implementing changes...',
        '[INFO] Reviewer agent: validating code quality...',
        '[SUCCESS] 3 issues found and fixed',
        '[INFO] Creating commit...',
        '[SUCCESS] Changes committed successfully'
    ],

    init() {
        this.commandEl = document.getElementById('typed-command');
        this.outputEl = document.getElementById('terminal-output');
        if (!this.commandEl || !this.outputEl) return;

        // Start typing effect after a delay
        setTimeout(() => this.typeCommand(), 1200);
    },

    async typeCommand() {
        const command = this.commands[Math.floor(Math.random() * this.commands.length)];
        this.commandEl.textContent = '';

        // Type each character with variable speed for realism
        for (let i = 0; i <= command.length; i++) {
            this.commandEl.textContent = command.slice(0, i);
            // Random typing speed between 30-80ms
            await this.sleep(30 + Math.random() * 50);
        }

        await this.sleep(400);
        await this.showOutput();
    },

    async showOutput() {
        this.outputEl.innerHTML = '';

        for (let i = 0; i < this.outputs.length; i++) {
            const line = document.createElement('div');
            line.className = 'terminal-output-line';
            line.textContent = this.outputs[i];
            line.style.animationDelay = `${i * 0.1}s`;
            this.outputEl.appendChild(line);

            // Variable delay between lines for realism
            await this.sleep(150 + Math.random() * 150);
        }

        // Reset after a longer delay
        await this.sleep(3500);
        this.commandEl.textContent = '';
        this.outputEl.innerHTML = '';
        await this.sleep(500);
        this.typeCommand();
    },

    sleep(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }
};

// === INSTALLATION TABS ===
const InstallTabs = {
    init() {
        this.tabs = document.querySelectorAll('.install-tab');
        this.panels = document.querySelectorAll('.install-panel');
        if (!this.tabs.length) return;

        this.tabs.forEach(tab => {
            tab.addEventListener('click', () => this.switchTab(tab));
        });
    },

    switchTab(activeTab) {
        const tabName = activeTab.dataset.tab;

        // Update tab states with smooth transition
        this.tabs.forEach(tab => {
            const isActive = tab === activeTab;
            tab.classList.toggle('install-tab--active', isActive);
            tab.setAttribute('aria-selected', isActive);
        });

        // Update panel visibility
        this.panels.forEach(panel => {
            const isActive = panel.dataset.panel === tabName;
            panel.classList.toggle('install-panel--active', isActive);
        });
    }
};

// === COPY TO CLIPBOARD ===
const CopyButton = {
    init() {
        document.querySelectorAll('.copy-btn').forEach(btn => {
            btn.addEventListener('click', () => this.copy(btn));
        });
    },

    async copy(btn) {
        const wrapper = btn.closest('.code-block-wrapper');
        const code = wrapper.querySelector('code');
        const text = code.textContent;

        try {
            await navigator.clipboard.writeText(text);
            this.showCopied(btn);
        } catch (err) {
            console.error('Failed to copy:', err);
            this.fallbackCopy(text);
            this.showCopied(btn);
        }
    },

    fallbackCopy(text) {
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.style.position = 'fixed';
        textarea.style.opacity = '0';
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
    },

    showCopied(btn) {
        const original = btn.innerHTML;
        btn.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="#39ff14" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6L9 17l-5-5"/></svg>';
        btn.classList.add('copied');

        setTimeout(() => {
            btn.innerHTML = original;
            btn.classList.remove('copied');
        }, 2000);
    }
};

// === PRESET MODE INTERACTION ===
const PresetModes = {
    init() {
        this.presets = document.querySelectorAll('.preset-card');
        if (!this.presets.length) return;

        this.presets.forEach(preset => {
            // Mouse tracking for spotlight effect
            preset.addEventListener('mousemove', (e) => this.updateSpotlight(e, preset));
            preset.addEventListener('mouseleave', () => this.clearSpotlight(preset));
            preset.addEventListener('click', () => this.selectPreset(preset));
        });
    },

    updateSpotlight(e, card) {
        const rect = card.getBoundingClientRect();
        const x = ((e.clientX - rect.left) / rect.width) * 100;
        const y = ((e.clientY - rect.top) / rect.height) * 100;
        card.style.setProperty('--mouse-x', `${x}%`);
        card.style.setProperty('--mouse-y', `${y}%`);
    },

    clearSpotlight(card) {
        card.style.removeProperty('--mouse-x');
        card.style.removeProperty('--mouse-y');
    },

    selectPreset(selected) {
        this.presets.forEach(preset => {
            preset.classList.toggle('preset-card--selected', preset === selected);
        });
    }
};

// === NAVIGATION HIGHLIGHT ON SCROLL ===
const NavHighlight = {
    sections: ['what', 'install', 'features', 'audience'],

    init() {
        if (Utils.prefersReducedMotion()) return;

        window.addEventListener('scroll', Utils.throttle(() => this.onScroll(), 100));
        this.onScroll();
    },

    onScroll() {
        const scrollPos = window.scrollY + 150;

        this.sections.forEach(id => {
            const section = document.getElementById(id);
            if (!section) return;

            const top = section.offsetTop;
            const bottom = top + section.offsetHeight;
            const link = document.querySelector(`.nav-link[href="#${id}"]`);

            if (scrollPos >= top && scrollPos < bottom) {
                document.querySelectorAll('.nav-link').forEach(l => l.classList.remove('nav-link--active'));
                link?.classList.add('nav-link--active');
            }
        });
    }
};

// === FLOW DIAGRAM ANIMATION ===
const FlowDiagram = {
    init() {
        this.steps = document.querySelectorAll('.flow-step');
        if (!this.steps.length || Utils.prefersReducedMotion()) return;

        const observer = new IntersectionObserver((entries) => {
            entries.forEach(entry => {
                if (entry.isIntersecting) {
                    this.animateSteps();
                    observer.unobserve(entry.target);
                }
            });
        }, { threshold: 0.4 });

        observer.observe(document.querySelector('.flow-diagram'));
    },

    animateSteps() {
        this.steps.forEach((step, index) => {
            setTimeout(() => {
                step.style.opacity = '0';
                step.style.transform = 'scale(0.9) translateY(10px)';
                step.style.transition = 'all 0.5s cubic-bezier(0.16, 1, 0.3, 1)';

                requestAnimationFrame(() => {
                    step.style.opacity = '1';
                    step.style.transform = 'scale(1) translateY(0)';
                });
            }, index * 150);
        });
    }
};

// === KEYBOARD NAVIGATION ===
const KeyboardNav = {
    init() {
        const tabList = document.querySelector('.install-tabs-nav');
        if (!tabList) return;

        tabList.addEventListener('keydown', (e) => {
            const tabs = [...tabList.querySelectorAll('.install-tab')];
            const currentIndex = tabs.indexOf(document.activeElement);

            if (e.key === 'ArrowRight') {
                e.preventDefault();
                const nextIndex = (currentIndex + 1) % tabs.length;
                tabs[nextIndex].focus();
                tabs[nextIndex].click();
            } else if (e.key === 'ArrowLeft') {
                e.preventDefault();
                const prevIndex = (currentIndex - 1 + tabs.length) % tabs.length;
                tabs[prevIndex].focus();
                tabs[prevIndex].click();
            }
        });
    }
};

// === MAGNETIC BUTTON EFFECT ===
const MagneticButtons = {
    init() {
        if (Utils.prefersReducedMotion()) return;

        document.querySelectorAll('.btn--primary').forEach(btn => {
            btn.addEventListener('mousemove', (e) => this.magneticEffect(e, btn));
            btn.addEventListener('mouseleave', (e) => this.resetButton(e, btn));
        });
    },

    magneticEffect(e, btn) {
        const rect = btn.getBoundingClientRect();
        const x = e.clientX - rect.left - rect.width / 2;
        const y = e.clientY - rect.top - rect.height / 2;

        // Subtle magnetic pull
        btn.style.transform = `translate(${x * 0.15}px, ${y * 0.15}px)`;
    },

    resetButton(e, btn) {
        btn.style.transform = '';
    }
};

// === PARALLAX EFFECT FOR GLOW SPOTS ===
const ParallaxGlow = {
    init() {
        if (Utils.prefersReducedMotion()) return;

        window.addEventListener('mousemove', Utils.throttle((e) => {
            const x = (e.clientX / window.innerWidth - 0.5) * 20;
            const y = (e.clientY / window.innerHeight - 0.5) * 20;

            document.querySelectorAll('.glow-spot').forEach((spot, index) => {
                const factor = (index + 1) * 0.5;
                spot.style.transform = `translate(${x * factor}px, ${y * factor}px)`;
            });
        }, 50));
    }
};

// === PERFORMANCE: Lazy load non-critical features ===
const LazyLoad = {
    init() {
        if ('requestIdleCallback' in window) {
            requestIdleCallback(() => {
                ScrollAnimations.init();
                FlowDiagram.init();
                MagneticButtons.init();
                ParallaxGlow.init();
            }, { timeout: 2000 });
        } else {
            setTimeout(() => {
                ScrollAnimations.init();
                FlowDiagram.init();
                MagneticButtons.init();
                ParallaxGlow.init();
            }, 200);
        }
    }
};

// === INITIALIZE ALL ===
document.addEventListener('DOMContentLoaded', () => {
    // Core functionality (always load)
    ThemeToggle.init();
    MobileNav.init();
    SmoothScroll.init();
    TerminalTyping.init();
    InstallTabs.init();
    CopyButton.init();
    PresetModes.init();
    NavHighlight.init();
    NavScroll.init();
    KeyboardNav.init();

    // Lazy load enhancements
    LazyLoad.init();
});

// === HANDLE RESIZE ===
let resizeTimer;
window.addEventListener('resize', () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => {
        if (window.innerWidth > 768) {
            MobileNav.closeMenu();
        }
    }, 250);
});

// === VISIBLE ON LOAD ===
// Ensure content is visible immediately
window.addEventListener('load', () => {
    document.body.classList.add('loaded');
});

// === CONSOLE EASTER EGG ===
console.log(`
%c
██   ██ ██    ██ ██████  ███████ ██████
 ██ ██  ██    ██ ██   ██ ██      ██   ██
  ███   ██    ██ ██████  █████   ██████
 ██ ██  ██    ██ ██      ██      ██   ██
██   ██  ██████  ██      ███████ ██   ██

%cAutomate your AI coding workflow.
https://codeberg.org/mistlight/ralph
`, 'font-family: monospace; font-size: 20px; color: #39ff14; font-weight: bold;', 'font-family: monospace; font-size: 12px; color: #8a8a8a;');
