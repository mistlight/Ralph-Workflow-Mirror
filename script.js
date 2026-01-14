/**
 * Ralph Workflow - Minimal JavaScript
 * Handles: mobile nav, install tabs, copy-to-clipboard, smooth scroll
 */

(function() {
    'use strict';

    // === Mobile Navigation ===
    const navToggle = document.querySelector('.nav-toggle');
    const navMenu = document.querySelector('.nav-menu');

    if (navToggle && navMenu) {
        navToggle.addEventListener('click', function() {
            const isOpen = navToggle.getAttribute('aria-expanded') === 'true';
            navToggle.setAttribute('aria-expanded', !isOpen);
            navMenu.style.display = isOpen ? 'none' : 'flex';
            navMenu.style.position = isOpen ? '' : 'absolute';
            navMenu.style.top = '100%';
            navMenu.style.left = '0';
            navMenu.style.right = '0';
            navMenu.style.flexDirection = 'column';
            navMenu.style.padding = '1.5rem';
            navMenu.style.background = 'rgba(250, 248, 243, 0.98)';
            navMenu.style.borderBottom = '1px solid var(--border-color)';
        });

        // Close menu when clicking a link
        navMenu.querySelectorAll('.nav-link').forEach(link => {
            link.addEventListener('click', () => {
                navToggle.setAttribute('aria-expanded', 'false');
                navMenu.style.display = 'none';
            });
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
                this.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>';
                setTimeout(() => {
                    this.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>';
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
                target.scrollIntoView({
                    behavior: 'smooth',
                    block: 'start'
                });
            }
        });
    });

    // === Active Nav Link on Scroll ===
    const sections = document.querySelectorAll('section[id]');
    const navLinks = document.querySelectorAll('.nav-link');

    function updateActiveNav() {
        let current = '';
        const scrollPos = window.scrollY + 100;

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

    window.addEventListener('scroll', updateActiveNav);
    updateActiveNav();

})();
