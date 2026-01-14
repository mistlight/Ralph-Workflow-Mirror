/**
 * Ralph Workflow - Website Interactions
 * Editorial aesthetic with smooth animations and refined interactions
 */

// === UTILITIES ===
const Utils = {
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

    prefersReducedMotion() {
        return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    }
};

// === MOBILE NAVIGATION ===
const MobileNav = {
    init() {
        this.toggle = document.querySelector('.nav-toggle');
        this.menu = document.querySelector('.nav-menu');
        if (!this.toggle || !this.menu) return;

        this.toggle.addEventListener('click', () => this.toggleMenu());

        this.menu.querySelectorAll('.nav-link').forEach(link => {
            link.addEventListener('click', () => this.closeMenu());
        });

        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') this.closeMenu();
        });

        document.addEventListener('click', (e) => {
            if (!e.target.closest('.nav')) this.closeMenu();
        });
    },

    toggleMenu() {
        const isOpen = this.toggle.getAttribute('aria-expanded') === 'true';
        this.toggle.setAttribute('aria-expanded', !isOpen);
        this.menu.setAttribute('aria-expanded', !isOpen);
        document.body.style.overflow = !isOpen ? 'hidden' : '';
    },

    closeMenu() {
        this.toggle.setAttribute('aria-expanded', 'false');
        this.menu.setAttribute('aria-expanded', 'false');
        document.body.style.overflow = '';
    }
};

// === SMOOTH SCROLL ===
const SmoothScroll = {
    init() {
        document.querySelectorAll('a[href^="#"]').forEach(anchor => {
            anchor.addEventListener('click', (e) => {
                const href = anchor.getAttribute('href');
                if (href === '#' || href === '') return;

                const target = document.querySelector(href);
                if (!target) return;

                e.preventDefault();
                const navHeight = document.querySelector('.nav')?.offsetHeight || 0;
                const targetPosition = target.getBoundingClientRect().top + window.pageYOffset - navHeight;

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
            threshold: 0.1,
            rootMargin: '0px 0px -50px 0px'
        };

        this.observer = new IntersectionObserver((entries) => {
            entries.forEach(entry => {
                if (entry.isIntersecting) {
                    entry.target.classList.add('animate-in');
                }
            });
        }, observerOptions);

        this.observeElements();
    },

    observeElements() {
        const selectors = [
            '.feature-card',
            '.preset-card',
            '.audience-card',
            '.workflow-step-item'
        ];

        selectors.forEach(selector => {
            document.querySelectorAll(selector).forEach((el, index) => {
                el.style.opacity = '0';
                el.style.transform = 'translateY(24px)';
                el.style.transition = 'opacity 0.6s cubic-bezier(0.16, 1, 0.3, 1), transform 0.6s cubic-bezier(0.16, 1, 0.3, 1)';
                const delay = (index % 6) * 0.1;
                el.style.transitionDelay = `${delay}s`;
                this.observer.observe(el);
            });
        });
    }
};

// Add animate-in styles
const animationStyles = document.createElement('style');
animationStyles.textContent = `
    .animate-in {
        opacity: 1 !important;
        transform: translateY(0) !important;
    }
`;
document.head.appendChild(animationStyles);

// === NAVIGATION SCROLL STATE ===
const NavScroll = {
    init() {
        this.nav = document.querySelector('.nav');
        if (!this.nav) return;

        window.addEventListener('scroll', Utils.throttle(() => {
            if (window.scrollY > 20) {
                this.nav.style.boxShadow = '0 2px 20px rgba(0, 0, 0, 0.05)';
            } else {
                this.nav.style.boxShadow = 'none';
            }
        }, 100), { passive: true });
    }
};

// === INSTALLATION TABS ===
const InstallTabs = {
    init() {
        this.tabs = document.querySelectorAll('.install-tab');
        if (!this.tabs.length) return;

        this.tabs.forEach(tab => {
            tab.addEventListener('click', () => this.switchTab(tab));
        });
    },

    switchTab(activeTab) {
        this.tabs.forEach(tab => {
            tab.classList.remove('install-tab-active');
            tab.setAttribute('aria-selected', 'false');
        });
        activeTab.classList.add('install-tab-active');
        activeTab.setAttribute('aria-selected', 'true');
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
        const codeBlock = btn.closest('.code-block');
        const code = codeBlock?.querySelector('code');
        if (!code) return;

        try {
            await navigator.clipboard.writeText(code.textContent);
            this.showCopied(btn);
        } catch (err) {
            this.fallbackCopy(code.textContent);
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
        btn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>';
        btn.style.color = '#28C840';

        setTimeout(() => {
            btn.innerHTML = original;
            btn.style.color = '';
        }, 2000);
    }
};

// === ACTIVE NAVIGATION HIGHLIGHT ===
const NavHighlight = {
    sections: ['how-it-works', 'install', 'features'],

    init() {
        window.addEventListener('scroll', Utils.throttle(() => this.onScroll(), 100));
        this.onScroll();
    },

    onScroll() {
        const scrollPos = window.scrollY + 100;

        this.sections.forEach(id => {
            const section = document.getElementById(id);
            if (!section) return;

            const top = section.offsetTop;
            const bottom = top + section.offsetHeight;
            const link = document.querySelector(`.nav-link[href="#${id}"]`);

            if (scrollPos >= top && scrollPos < bottom) {
                document.querySelectorAll('.nav-link').forEach(l => l.classList.remove('nav-link-active'));
                link?.classList.add('nav-link-active');
            }
        });
    }
};

// === PRESET CARD INTERACTIONS ===
const PresetCards = {
    init() {
        this.presets = document.querySelectorAll('.preset-card');
        if (!this.presets.length) return;

        this.presets.forEach(card => {
            card.addEventListener('click', () => this.selectPreset(card));
        });
    },

    selectPreset(selected) {
        this.presets.forEach(preset => {
            preset.classList.toggle('preset-card-selected', preset === selected);
        });
    }
};

// === HANDLE RESIZE ===
let resizeTimer;
window.addEventListener('resize', () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => {
        if (window.innerWidth > 768) {
            MobileNav.closeMenu?.();
        }
    }, 250);
});

// === TERMINAL TYPING EFFECT ===
const TerminalTyping = {
    init() {
        const terminalOutput = document.querySelector('.terminal-output');
        if (!terminalOutput) return;

        const lines = terminalOutput.querySelectorAll('.terminal-output-line');
        if (lines.length === 0) return;

        // Store original text and clear
        lines.forEach(line => {
            line.dataset.original = line.innerHTML;
            line.style.opacity = '0';
        });

        // Animate each line appearing
        let delay = 500;
        lines.forEach((line, index) => {
            setTimeout(() => {
                line.style.transition = 'opacity 0.3s ease';
                line.style.opacity = '1';
            }, delay);
            delay += 400;
        });
    }
};

// === HERO INITIAL ANIMATION ===
const HeroAnimation = {
    init() {
        if (Utils.prefersReducedMotion()) return;

        const hero = document.querySelector('.hero-primary');
        if (!hero) return;

        const elements = [
            hero.querySelector('.hero-meta'),
            hero.querySelector('.hero-title'),
            hero.querySelector('.hero-description'),
            hero.querySelector('.hero-actions')
        ].filter(Boolean);

        elements.forEach((el, index) => {
            el.style.opacity = '0';
            el.style.transform = 'translateY(20px)';
        });

        // Stagger reveal
        setTimeout(() => {
            elements.forEach((el, index) => {
                setTimeout(() => {
                    el.style.transition = 'opacity 0.8s cubic-bezier(0.16, 1, 0.3, 1), transform 0.8s cubic-bezier(0.16, 1, 0.3, 1)';
                    el.style.opacity = '1';
                    el.style.transform = 'translateY(0)';
                }, index * 150);
            });
        }, 200);

        // Terminal visual
        const terminal = document.querySelector('.hero-visual');
        if (terminal) {
            terminal.style.opacity = '0';
            terminal.style.transform = 'perspective(1000px) rotateY(-10deg) translateX(50px)';
            setTimeout(() => {
                terminal.style.transition = 'opacity 1s ease, transform 1s cubic-bezier(0.16, 1, 0.3, 1)';
                terminal.style.opacity = '1';
                terminal.style.transform = 'perspective(1000px) rotateY(-2deg) rotateX(1deg) translateX(0)';
            }, 600);
        }
    }
};

// === INITIALIZE ALL ===
document.addEventListener('DOMContentLoaded', () => {
    MobileNav.init();
    SmoothScroll.init();
    ScrollAnimations.init();
    NavScroll.init();
    InstallTabs.init();
    CopyButton.init();
    NavHighlight.init();
    PresetCards.init();
    TerminalTyping.init();
    HeroAnimation.init();
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
`, 'font-family: monospace; font-size: 16px; color: #C45C26; font-weight: bold;', 'font-family: monospace; font-size: 12px; color: #6B6B7B;');
