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
            '.workflow-step-item',
            '.timeline-step',
            '.what-key-point'
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
        const tabName = activeTab.dataset.tab;

        // Update tab styles
        this.tabs.forEach(tab => {
            tab.classList.remove('install-tab-active');
            tab.setAttribute('aria-selected', 'false');
        });
        activeTab.classList.add('install-tab-active');
        activeTab.setAttribute('aria-selected', 'true');

        // Update content visibility
        document.querySelectorAll('.install-content').forEach(content => {
            content.classList.remove('install-content-active');
            if (content.dataset.content === tabName) {
                content.classList.add('install-content-active');
            }
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
        btn.classList.add('success');
        const originalHTML = btn.innerHTML;
        btn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>';

        setTimeout(() => {
            btn.classList.remove('success');
            btn.innerHTML = originalHTML;
        }, 2000);
    }
};

// === ACTIVE NAVIGATION HIGHLIGHT ===
const NavHighlight = {
    sections: ['what-is-ralph', 'how-it-works', 'install', 'features'],

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

// === MAGNETIC BUTTON EFFECT ===
const MagneticButtons = {
    init() {
        const buttons = document.querySelectorAll('.btn');
        if (Utils.prefersReducedMotion()) return;

        buttons.forEach(btn => {
            btn.addEventListener('mousemove', (e) => this.onMouseMove(e, btn));
            btn.addEventListener('mouseleave', (e) => this.onMouseLeave(e, btn));
        });
    },

    onMouseMove(e, btn) {
        const rect = btn.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const y = e.clientY - rect.top;

        const centerX = rect.width / 2;
        const centerY = rect.height / 2;

        const deltaX = (x - centerX) / centerX;
        const deltaY = (y - centerY) / centerY;

        const moveX = deltaX * 3;
        const moveY = deltaY * 3;

        btn.style.transform = `translate(${moveX}px, ${moveY}px)`;
    },

    onMouseLeave(e, btn) {
        btn.style.transform = '';
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
            hero.querySelector('.hero-actions'),
            hero.querySelector('.hero-trust')
        ].filter(Boolean);

        elements.forEach((el, index) => {
            el.style.opacity = '0';
            el.style.transform = 'translateY(30px)';
        });

        // Stagger reveal with spring physics
        setTimeout(() => {
            elements.forEach((el, index) => {
                setTimeout(() => {
                    el.style.transition = 'opacity 0.9s cubic-bezier(0.16, 1, 0.3, 1), transform 0.9s cubic-bezier(0.34, 1.56, 0.64, 1)';
                    el.style.opacity = '1';
                    el.style.transform = 'translateY(0)';
                }, index * 120);
            });
        }, 150);

        // Agent orchestration visualization with enhanced animation
        const orchestration = document.querySelector('.agent-orchestration');
        if (orchestration) {
            orchestration.style.opacity = '0';
            orchestration.style.transform = 'perspective(1000px) rotateY(-15deg) rotateX(10deg) translateX(80px) translateY(40px)';
            setTimeout(() => {
                orchestration.style.transition = 'opacity 1.2s ease, transform 1.2s cubic-bezier(0.16, 1, 0.3, 1)';
                orchestration.style.opacity = '1';
                orchestration.style.transform = 'perspective(1000px) rotateY(-2deg) rotateX(1deg) translateX(0) translateY(0)';
            }, 400);
        }

        // Animate glows
        const glows = document.querySelectorAll('.hero-glow');
        glows.forEach((glow, index) => {
            glow.style.opacity = '0';
            setTimeout(() => {
                glow.style.transition = 'opacity 1.5s ease';
                glow.style.opacity = '';
            }, 800 + (index * 200));
        });

        // Start agent animation simulation
        AgentAnimation.init();
    }
};

// === AGENT ORCHESTRATION ANIMATION ===
const AgentAnimation = {
    init() {
        const devAgent = document.querySelector('.agent-node-dev');
        const reviewAgent = document.querySelector('.agent-node-review');
        const devStatus = devAgent?.querySelector('.agent-status');
        const reviewStatus = reviewAgent?.querySelector('.agent-status');

        if (!devStatus || !reviewStatus) return;

        // Simulate agent activity cycle
        const states = [
            { dev: 'Writing code...', review: 'Awaiting code...' },
            { dev: 'Code complete!', review: 'Reviewing...' },
            { dev: 'Fixing issues...', review: 'Found bugs...' },
            { dev: 'Writing code...', review: 'Awaiting code...' },
            { dev: 'Code complete!', review: 'Approved ✓' }
        ];

        let currentState = 0;

        // Cycle through states
        setInterval(() => {
            currentState = (currentState + 1) % states.length;
            const state = states[currentState];

            devStatus.textContent = state.dev;
            reviewStatus.textContent = state.review;

            // Update status classes
            if (state.dev.includes('complete')) {
                devStatus.classList.remove('agent-status-active');
                reviewStatus.classList.add('agent-status-active');
            } else if (state.dev.includes('Writing') || state.dev.includes('Fixing')) {
                devStatus.classList.add('agent-status-active');
                reviewStatus.classList.remove('agent-status-active');
            }

            // Flash effect on state change
            devAgent.style.transform = 'scale(1.02)';
            setTimeout(() => {
                devAgent.style.transform = '';
            }, 150);
        }, 3000);
    }
};

// === PLATFORM DETECTION ===
const PlatformDetection = {
    init() {
        const banner = document.getElementById('platformBanner');
        const icon = document.getElementById('platformIcon');
        const name = document.getElementById('platformName');

        if (!banner || !icon || !name) return;

        const platform = this.detectPlatform();
        if (platform) {
            name.textContent = platform.name;
            icon.innerHTML = platform.icon;
            banner.style.display = 'flex';

            // Auto-select the appropriate tab
            setTimeout(() => {
                const tabs = document.querySelectorAll('.install-tab');
                tabs.forEach(tab => {
                    if (tab.dataset.tab === platform.recommendedTab) {
                        tab.click();
                    }
                });
            }, 500);
        }
    },

    detectPlatform() {
        const ua = navigator.userAgent.toLowerCase();
        const platform = navigator.platform.toLowerCase();

        // macOS
        if (platform.includes('mac') || ua.includes('mac os x')) {
            return {
                name: 'macOS',
                icon: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.81-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z"/></svg>',
                recommendedTab: 'cargo'
            };
        }

        // Windows
        if (platform.includes('win') || ua.includes('windows')) {
            return {
                name: 'Windows',
                icon: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M3 12V6.5L8.5 3.5V9H11V12H8.5V19.5L3 16.5V12ZM12.5 12V6.5L18 3.5V9H20.5V12H18V19.5L12.5 16.5V12Z"/></svg>',
                recommendedTab: 'cargo'
            };
        }

        // Linux
        if (platform.includes('linux') || ua.includes('linux')) {
            return {
                name: 'Linux',
                icon: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 2c1.85 0 3.55.54 5 1.43-.38.92-.73 2.08-.96 3.23-.6.08-1.23.13-1.87.13-.48 0-.95-.03-1.41-.1.15-1.31.33-2.58.56-3.76.12-.61.27-1.21.43-1.8C13.23 4.3 12.63 4 12 4zM5.65 6.37C7.02 5.15 8.82 4.38 10.76 4.12c-.16.59-.31 1.19-.43 1.8-.24 1.18-.41 2.45-.56 3.76-1.37.24-2.68.68-3.89 1.28-.03-1.25-.08-2.59-.08-3.99 0-.55.02-1.08.06-1.6h-.21zm12.7 0c.04.52.06 1.05.06 1.6 0 1.4-.05 2.74-.08 3.99-1.21-.6-2.52-1.04-3.89-1.28-.15-1.31-.32-2.58-.56-3.76-.12-.61-.27-1.21-.43-1.8 1.94.26 3.74 1.03 5.11 2.25h-.21zM6.8 11.16c1.17-.57 2.43-.99 3.75-1.22-.13 1.27-.23 2.57-.29 3.89-.01.01-.01.01-.01.02-1.46.18-2.86.5-4.18.94.24-1.25.44-2.46.59-3.63h.14zm10.4 0c.15 1.17.35 2.38.59 3.63-1.32-.44-2.72-.76-4.18-.94 0-.01 0-.01-.01-.02-.06-1.32-.16-2.62-.29-3.89 1.32.23 2.58.65 3.75 1.22h.14zM10.2 13.92c.06-1.25.15-2.49.27-3.7 1.22.2 2.38.2 3.6 0 .12 1.21.21 2.45.27 3.7-1.38-.19-2.76-.19-4.14 0zm2.07 1.08c.06 1.27.16 2.57.29 3.84-1.32-.23-2.58-.65-3.75-1.22-.15-1.17-.35-2.38-.59-3.63 1.32.44 2.72.76 4.18.94.01.01.01.01.01.02h-.14zm-.14 1.02c-.15-1.31-.32-2.58-.56-3.76-1.22.24-2.52.24-3.76 0-.24 1.18-.41 2.45-.56 3.76.48.07.95.1 1.41.1.64 0 1.27-.05 1.87-.13h.6z"/></svg>',
                recommendedTab: 'cargo'
            };
        }

        return null;
    }
};

// === WORKFLOW DEMO ===
const WorkflowDemo = {
    init() {
        const container = document.getElementById('workflowDemo');
        if (!container) return;

        const steps = container.querySelectorAll('.demo-step');
        const panels = document.querySelectorAll('.demo-panel');

        steps.forEach(step => {
            step.addEventListener('click', () => {
                const stepNum = step.dataset.step;

                // Update step buttons
                steps.forEach(s => s.classList.remove('demo-step-active'));
                step.classList.add('demo-step-active');

                // Update panels
                panels.forEach(panel => {
                    panel.classList.remove('demo-panel-active');
                    if (panel.dataset.panel === stepNum) {
                        panel.classList.add('demo-step-active');
                    }
                });
            });
        });
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
    MagneticButtons.init();
    TerminalTyping.init();
    HeroAnimation.init();
    PlatformDetection.init();
    WorkflowDemo.init();
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
`, 'font-family: monospace; font-size: 16px; color: #FFB800; font-weight: bold;', 'font-family: monospace; font-size: 12px; color: #8A8A8A;');
