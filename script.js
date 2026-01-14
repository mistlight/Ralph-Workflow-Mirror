/**
 * Ralph Workflow - Enhanced JavaScript
 * Handles: mobile nav, install tabs, copy-to-clipboard, smooth scroll, scroll animations,
 * terminal typing effect, nav scroll detection, parallax effects
 */

(function() {
    'use strict';

    // === Parallax Effect for Hero Glows ===
    const heroGlows = document.querySelectorAll('.hero-glow, .hero-glow-2, .hero-glow-3');

    function updateParallax() {
        const scrollY = window.scrollY;
        const windowHeight = window.innerHeight;

        if (scrollY < windowHeight * 1.5) {
            const parallaxValue = scrollY * 0.15;

            heroGlows.forEach((glow, index) => {
                const factor = (index + 1) * 0.5;
                glow.style.transform = `translate(${parallaxValue * factor}px, ${parallaxValue * factor * 0.5}px)`;
            });
        }
    }

    // Throttled scroll handler for parallax
    let parallaxTicking = false;
    window.addEventListener('scroll', () => {
        if (!parallaxTicking) {
            window.requestAnimationFrame(() => {
                updateParallax();
                parallaxTicking = false;
            });
            parallaxTicking = true;
        }
    });

    // === Navigation Scroll Effect ===
    const nav = document.querySelector('.nav');
    let lastScrollY = 0;

    function updateNav() {
        const scrollY = window.scrollY;

        if (scrollY > 50) {
            nav.classList.add('scrolled');
        } else {
            nav.classList.remove('scrolled');
        }

        lastScrollY = scrollY;
    }

    // Throttled scroll handler for nav
    let navTicking = false;
    window.addEventListener('scroll', () => {
        if (!navTicking) {
            window.requestAnimationFrame(() => {
                updateNav();
                navTicking = false;
            });
            navTicking = true;
        }
    });

    // Initial call
    updateNav();

    // === Terminal Typing Effect ===
    const terminalLines = document.querySelectorAll('.terminal-line');
    const terminal = document.querySelector('.terminal-body');

    function animateTerminal() {
        if (!terminal) return;

        // Reset all lines
        terminalLines.forEach(line => {
            line.classList.remove('typed');
        });

        // More realistic timing - first lines faster, slower for output
        const timing = [600, 1200, 1500, 1800, 2200, 2800, 3500];

        // Animate lines sequentially
        terminalLines.forEach((line, index) => {
            setTimeout(() => {
                line.classList.add('typed');
            }, timing[index] || index * 600);
        });
    }

    // Run animation on page load
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', animateTerminal);
    } else {
        setTimeout(animateTerminal, 500);
    }

    // === Mobile Navigation ===
    const navToggle = document.querySelector('.nav-toggle');
    const navMenu = document.querySelector('.nav-menu');

    if (navToggle && navMenu) {
        navToggle.addEventListener('click', function() {
            const isOpen = navToggle.getAttribute('aria-expanded') === 'true';
            navToggle.setAttribute('aria-expanded', !isOpen);

            if (isOpen) {
                navMenu.classList.remove('mobile-open');
                document.body.style.overflow = '';
            } else {
                navMenu.classList.add('mobile-open');
                document.body.style.overflow = 'hidden';
            }
        });

        // Close menu when clicking a link
        navMenu.querySelectorAll('.nav-link').forEach(link => {
            link.addEventListener('click', () => {
                navToggle.setAttribute('aria-expanded', 'false');
                navMenu.classList.remove('mobile-open');
                document.body.style.overflow = '';
            });
        });

        // Close menu when clicking outside
        document.addEventListener('click', (e) => {
            if (!navToggle.contains(e.target) && !navMenu.contains(e.target)) {
                navToggle.setAttribute('aria-expanded', 'false');
                navMenu.classList.remove('mobile-open');
                document.body.style.overflow = '';
            }
        });

        // Close menu on escape key
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && navMenu.classList.contains('mobile-open')) {
                navToggle.setAttribute('aria-expanded', 'false');
                navMenu.classList.remove('mobile-open');
                document.body.style.overflow = '';
                navToggle.focus();
            }
        });
    }

    // === Install Tabs ===
    const installTabs = document.querySelectorAll('.install-tab');
    const installContents = document.querySelectorAll('.install-content');

    installTabs.forEach(tab => {
        tab.addEventListener('click', function() {
            const targetTab = this.dataset.tab;

            // Update active tab
            installTabs.forEach(t => t.classList.remove('install-tab-active'));
            this.classList.add('install-tab-active');

            // Show corresponding content
            installContents.forEach(content => {
                if (content.dataset.content === targetTab) {
                    content.style.display = 'block';
                    content.style.opacity = '0';
                    content.style.transform = 'translateY(10px)';

                    // Trigger reflow for animation
                    content.offsetHeight;

                    content.style.transition = 'opacity 0.3s ease, transform 0.3s ease';
                    content.style.opacity = '1';
                    content.style.transform = 'translateY(0)';
                } else {
                    content.style.display = 'none';
                }
            });
        });
    });

    // === Copy to Clipboard ===
    const copyButtons = document.querySelectorAll('.copy-btn');

    copyButtons.forEach(btn => {
        btn.addEventListener('click', async function() {
            const codeBlock = this.closest('.code-block');
            const code = codeBlock.querySelector('code').textContent;

            try {
                await navigator.clipboard.writeText(code);

                // Show success state with animation
                const originalHTML = this.innerHTML;
                this.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>';
                this.classList.add('copied');

                setTimeout(() => {
                    this.innerHTML = originalHTML;
                    this.classList.remove('copied');
                }, 2000);
            } catch (err) {
                console.error('Failed to copy:', err);
            }
        });
    });

    // === Smooth Scroll ===
    document.querySelectorAll('a[href^="#"]').forEach(anchor => {
        anchor.addEventListener('click', function(e) {
            const targetId = this.getAttribute('href');
            if (targetId === '#') return;

            const target = document.querySelector(targetId);
            if (target) {
                e.preventDefault();
                const navHeight = document.querySelector('.nav')?.offsetHeight || 0;
                const targetPosition = target.offsetTop - navHeight - 20;

                window.scrollTo({
                    top: targetPosition,
                    behavior: 'smooth'
                });
            }
        });
    });

    // === Active Nav Link on Scroll ===
    const sections = document.querySelectorAll('section[id]');
    const navLinks = document.querySelectorAll('.nav-link');

    function updateActiveNav() {
        let current = '';
        const scrollPos = window.scrollY + 150;

        sections.forEach(section => {
            const sectionTop = section.offsetTop;
            const sectionHeight = section.offsetHeight;

            if (scrollPos >= sectionTop && scrollPos < sectionTop + sectionHeight) {
                current = section.getAttribute('id');
            }
        });

        navLinks.forEach(link => {
            link.classList.remove('nav-link-active');
            if (link.getAttribute('href') === `#${current}`) {
                link.classList.add('nav-link-active');
            }
        });
    }

    // Throttled scroll handler
    let ticking = false;
    window.addEventListener('scroll', () => {
        if (!ticking) {
            window.requestAnimationFrame(() => {
                updateActiveNav();
                ticking = false;
            });
            ticking = true;
        }
    });

    // Initial call
    updateActiveNav();

    // === Scroll Animations (Intersection Observer) ===
    const observerOptions = {
        root: null,
        rootMargin: '0px 0px -100px 0px',
        threshold: 0.1
    };

    const animationObserver = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                entry.target.classList.add('fade-in');
                entry.target.style.opacity = '1';
                entry.target.style.transform = 'translateY(0)';
                animationObserver.unobserve(entry.target);
            }
        });
    }, observerOptions);

    // Observe elements for animation
    const animatedElements = document.querySelectorAll(
        '.workflow-step, .feature-card, .audience-card, .key-point, .section-header'
    );

    animatedElements.forEach((el, index) => {
        el.style.opacity = '0';
        el.style.transform = 'translateY(30px)';
        el.style.transition = 'opacity 0.6s ease, transform 0.6s ease';
        el.style.transitionDelay = `${index * 0.05}s`;
        animationObserver.observe(el);
    });

    // === Reduced Motion Support ===
    const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

    function handleReducedMotion() {
        if (prefersReducedMotion.matches) {
            // Disable scroll animations
            animatedElements.forEach(el => {
                el.style.opacity = '1';
                el.style.transform = 'none';
                el.style.transition = 'none';
            });

            // Use instant scroll instead of smooth
            document.querySelectorAll('a[href^="#"]').forEach(anchor => {
                anchor.style.scrollBehavior = 'auto';
            });

            // Disable parallax
            if (heroGlows.length > 0) {
                heroGlows.forEach(glow => {
                    glow.style.transform = '';
                });
            }
        }
    }

    handleReducedMotion();
    prefersReducedMotion.addEventListener('change', handleReducedMotion);

    // === Platform Detection for Install Tabs (Optional Enhancement) ===
    function detectPlatform() {
        const platform = navigator.platform.toLowerCase();
        let defaultTab = 'cargo';

        if (platform.includes('win')) {
            // Windows users might prefer source install
            defaultTab = 'source';
        } else if (platform.includes('mac') || platform.includes('linux')) {
            // Mac/Linux users can use cargo
            defaultTab = 'cargo';
        }

        // Click the default tab
        const defaultTabButton = document.querySelector(`[data-tab="${defaultTab}"]`);
        if (defaultTabButton && !document.querySelector('.install-tab-active')) {
            defaultTabButton.click();
        }
    }

    // Only auto-select on first visit
    if (!localStorage.getItem('ralph-tab-selected')) {
        detectPlatform();
        localStorage.setItem('ralph-tab-selected', 'true');
    }

})();
