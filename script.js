/**
 * Ralph Workflow - Enhanced JavaScript
 * Handles: mobile nav, install tabs, copy-to-clipboard, smooth scroll, scroll animations,
 * terminal typing effect, nav scroll detection, parallax effects, magnetic buttons
 */

(function() {
    'use strict';

    // === Magnetic Button Effect ===
    const buttons = document.querySelectorAll('.btn');

    buttons.forEach(btn => {
        btn.addEventListener('mousemove', function(e) {
            const rect = btn.getBoundingClientRect();
            const x = e.clientX - rect.left - rect.width / 2;
            const y = e.clientY - rect.top - rect.height / 2;

            btn.style.transform = `translate(${x * 0.15}px, ${y * 0.15}px)`;
        });

        btn.addEventListener('mouseleave', function() {
            btn.style.transform = '';
        });
    });

    // === Enhanced Parallax Effect for Hero Glows ===
    const heroGlows = document.querySelectorAll('.hero-glow, .hero-glow-2, .hero-glow-3, .hero-glow-4, .hero-glow-5');

    function updateParallax() {
        const scrollY = window.scrollY;
        const windowHeight = window.innerHeight;

        if (scrollY < windowHeight * 1.5) {
            const parallaxValue = scrollY * 0.15;

            heroGlows.forEach((glow, index) => {
                const factor = (index + 1) * 0.6;
                const rotation = (index + 1) * 0.05;
                glow.style.transform = `translate(${parallaxValue * factor}px, ${parallaxValue * factor * 0.5}px) rotate(${rotation}deg)`;
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

    // === Terminal Control Buttons ===
    const terminalPlayPause = document.getElementById('terminal-play-pause');
    const terminalRestart = document.getElementById('terminal-restart');
    const terminalRunDemo = document.getElementById('terminal-run-demo');
    const speedButtons = document.querySelectorAll('.terminal-speed-btn');
    const speedLabel = document.getElementById('speed-label');
    const progressBar = document.getElementById('progress-bar');

    let isPlaying = true;
    let animationSpeed = 1;
    let animationTimeouts = [];

    // Store original timeouts for speed control
    const originalTiming = [600, 1200, 1500, 1800, 2200, 2800, 3500];

    function clearTerminalAnimations() {
        animationTimeouts.forEach(timeout => clearTimeout(timeout));
        animationTimeouts = [];
    }

    function getAdjustedTiming() {
        return originalTiming.map(t => t / animationSpeed);
    }

    // Play/Pause functionality
    if (terminalPlayPause) {
        terminalPlayPause.addEventListener('click', () => {
            isPlaying = !isPlaying;

            const iconPause = terminalPlayPause.querySelector('.icon-pause');
            const iconPlay = terminalPlayPause.querySelector('.icon-play');

            if (isPlaying) {
                iconPause.style.display = 'block';
                iconPlay.style.display = 'none';
                terminalPlayPause.setAttribute('aria-label', 'Pause animation');
                animateTerminal();
            } else {
                iconPause.style.display = 'none';
                iconPlay.style.display = 'block';
                terminalPlayPause.setAttribute('aria-label', 'Play animation');
                clearTerminalAnimations();
            }
        });
    }

    // Restart functionality
    if (terminalRestart) {
        terminalRestart.addEventListener('click', () => {
            clearTerminalAnimations();
            terminalLines.forEach(line => {
                line.classList.remove('typed');
            });

            // Reset progress bar
            if (progressBar) {
                progressBar.style.transition = 'none';
                progressBar.style.width = '0%';
                setTimeout(() => {
                    progressBar.style.transition = 'width 4s ease-out';
                }, 50);
            }

            animateTerminal();
        });
    }

    // Speed control functionality
    speedButtons.forEach(btn => {
        btn.addEventListener('click', () => {
            const speed = parseFloat(btn.dataset.speed);
            animationSpeed = speed;

            // Update active state
            speedButtons.forEach(b => b.classList.remove('active'));
            btn.classList.add('active');

            // Update label
            if (speedLabel) {
                speedLabel.textContent = `${speed}x`;
            }

            // If currently paused, restart with new speed
            if (!isPlaying) {
                clearTerminalAnimations();
                terminalLines.forEach(line => {
                    line.classList.remove('typed');
                });
                animateTerminal();
            }
        });
    });

    // Run Full Demo functionality
    if (terminalRunDemo) {
        terminalRunDemo.addEventListener('click', () => {
            // Clear any existing animations
            clearTerminalAnimations();
            terminalLines.forEach(line => {
                line.classList.remove('typed');
            });

            // Reset progress bar
            if (progressBar) {
                progressBar.style.transition = 'none';
                progressBar.style.width = '0%';
                setTimeout(() => {
                    progressBar.style.transition = `width ${4 / animationSpeed}s ease-out`;
                    progressBar.style.width = '100%';
                }, 50);
            }

            // Create extended demo content
            const demoSteps = [
                { text: 'ralph -S', delay: 0, type: 'command' },
                { text: 'Reading PROMPT.md', delay: 600, type: 'output' },
                { text: 'Developer agent: Analyzing requirements...', delay: 1500, type: 'output' },
                { text: 'Developer agent: Writing feature implementation...', delay: 3000, type: 'output' },
                { text: 'Reviewer agent: Checking code quality...', delay: 5000, type: 'output' },
                { text: 'Reviewer agent: Found 2 issues, requesting fixes...', delay: 6500, type: 'output' },
                { text: 'Developer agent: Applying fixes...', delay: 8000, type: 'output' },
                { text: 'Reviewer agent: Validating fixes...', delay: 9500, type: 'output' },
                { text: 'Iteration 2/10 complete', delay: 11000, type: 'output' },
                { text: 'Developer agent: Running tests...', delay: 12500, type: 'output' },
                { text: 'All tests passed!', delay: 14000, type: 'output' },
                { text: 'Code approved, creating commit...', delay: 15500, type: 'success' },
                { text: 'Changes shipped in 3m 42s', delay: 17000, type: 'time' }
            ];

            // Run demo with extended content
            demoSteps.forEach((step, index) => {
                const timeout = setTimeout(() => {
                    if (step.type === 'command') {
                        const commandLine = terminalLines[0];
                        commandLine.querySelector('.terminal-command').textContent = step.text;
                        commandLine.classList.add('typed');
                    } else {
                        // For demo, we'll just animate the existing lines
                        if (index < terminalLines.length - 1) {
                            const line = terminalLines[index + 1];
                            if (line) {
                                line.classList.add('typed');
                                // Update text for demo
                                if (step.text) {
                                    const textElement = line.querySelector('.terminal-output-text');
                                    const iconElement = line.querySelector('.terminal-output-icon');
                                    if (textElement) textElement.textContent = step.text;
                                    if (iconElement && step.type === 'success') {
                                        iconElement.textContent = '✓';
                                        iconElement.classList.add('terminal-icon-success');
                                    }
                                }
                            }
                        }
                    }
                }, step.delay / animationSpeed);

                animationTimeouts.push(timeout);
            });

            // Update button text during demo
            terminalRunDemo.disabled = true;
            terminalRunDemo.style.opacity = '0.5';
            terminalRunDemo.innerHTML = `
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="margin-right: 6px;">
                    <circle cx="12" cy="12" r="10"></circle>
                    <polyline points="12 6 12 12 16 14"></polyline>
                </svg>
                Running...
            `;

            // Reset button after demo
            setTimeout(() => {
                terminalRunDemo.disabled = false;
                terminalRunDemo.style.opacity = '1';
                terminalRunDemo.innerHTML = `
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="margin-right: 6px;">
                        <polygon points="5 3 19 12 5 21 5 3"></polygon>
                    </svg>
                    Run Full Demo
                `;
            }, 18000 / animationSpeed);
        });
    }

    // Progress bar animation
    function animateProgressBar() {
        if (progressBar) {
            progressBar.style.transition = `width ${4 / animationSpeed}s ease-out`;
            progressBar.style.width = '100%';
        }
    }

    // Trigger progress bar with terminal animation
    const originalAnimateTerminal = animateTerminal;
    animateTerminal = function() {
        if (!terminal) return;

        // Reset all lines
        terminalLines.forEach(line => {
            line.classList.remove('typed');
        });

        // Animate progress bar
        animateProgressBar();

        // Get adjusted timing based on current speed
        const timing = getAdjustedTiming();

        // Animate lines sequentially
        terminalLines.forEach((line, index) => {
            const timeout = setTimeout(() => {
                line.classList.add('typed');
            }, timing[index] || index * (600 / animationSpeed));

            animationTimeouts.push(timeout);
        });
    };


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

    // === Install Mode Toggle (Simple/Advanced) ===
    const installModeSwitch = document.getElementById('install-mode-switch');
    const installSection = document.getElementById('install');
    const simpleTabs = document.querySelectorAll('.install-tab-simple');
    const advancedTabs = document.querySelectorAll('.install-tab-advanced');
    const advancedRequirements = document.querySelector('.install-requirements');

    // Check localStorage for saved preference
    const savedMode = localStorage.getItem('ralph-install-mode');
    if (savedMode === 'advanced') {
        installSection?.setAttribute('data-mode', 'advanced');
        installModeSwitch?.setAttribute('aria-checked', 'true');
        // Show advanced tabs
        advancedTabs.forEach(tab => tab.style.display = '');
        // Hide simple tabs (or keep first one)
        simpleTabs.forEach((tab, index) => {
            if (index > 0) tab.style.display = 'none';
        });
    } else {
        installSection?.setAttribute('data-mode', 'simple');
        installModeSwitch?.setAttribute('aria-checked', 'false');
        // Hide advanced tabs
        advancedTabs.forEach(tab => tab.style.display = 'none');
        // Show simple tabs
        simpleTabs.forEach(tab => tab.style.display = '');
    }

    if (installModeSwitch) {
        installModeSwitch.addEventListener('click', () => {
            const isAdvanced = installModeSwitch.getAttribute('aria-checked') === 'true';
            const newMode = isAdvanced ? 'simple' : 'advanced';

            // Update state
            installModeSwitch.setAttribute('aria-checked', !isAdvanced);
            installSection?.setAttribute('data-mode', newMode);

            // Save preference
            localStorage.setItem('ralph-install-mode', newMode);

            // Toggle tabs visibility
            if (newMode === 'advanced') {
                advancedTabs.forEach(tab => {
                    tab.style.display = '';
                    tab.style.opacity = '0';
                    setTimeout(() => tab.style.opacity = '1', 50);
                });
                simpleTabs.forEach((tab, index) => {
                    if (index > 0) tab.style.display = 'none';
                });

                // Show full requirements in advanced mode
                if (advancedRequirements) {
                    const allRequirements = advancedRequirements.querySelectorAll('.requirement');
                    allRequirements.forEach(req => req.style.display = '');
                }
            } else {
                advancedTabs.forEach(tab => tab.style.display = 'none');
                simpleTabs.forEach(tab => {
                    tab.style.display = '';
                    tab.style.opacity = '0';
                    setTimeout(() => tab.style.opacity = '1', 50);
                });

                // In simple mode, only show basic requirement
                if (advancedRequirements) {
                    const allRequirements = advancedRequirements.querySelectorAll('.requirement');
                    allRequirements.forEach((req, index) => {
                        if (index > 0) req.style.display = 'none';
                    });
                }

                // Switch back to simple tab if currently on advanced tab
                const activeTab = document.querySelector('.install-tab-active');
                if (activeTab && activeTab.classList.contains('install-tab-advanced')) {
                    const simpleTab = document.querySelector('.install-tab-simple');
                    if (simpleTab) simpleTab.click();
                }
            }
        });
    }

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

    // === Enhanced Scroll Animations (Intersection Observer) ===
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

    // === Kinetic Typography on Scroll ===
    const heroWords = document.querySelectorAll('.hero-word');
    let kineticScrollY = window.scrollY;

    function updateKineticTypography() {
        const scrollY = window.scrollY;
        const scrollDelta = scrollY - kineticScrollY;
        const heroSection = document.querySelector('.hero');

        if (!heroSection) return;

        const heroRect = heroSection.getBoundingClientRect();
        const heroVisible = heroRect.bottom > 0 && heroRect.top < window.innerHeight;

        if (heroVisible) {
            heroWords.forEach((word, index) => {
                const speed = (index + 1) * 0.02;
                const yPos = scrollDelta * speed;
                const currentTransform = word.style.transform || 'translateY(0) scale(1)';
                const match = currentTransform.match(/translateY\(([^)]+)\)/);
                const currentY = match ? parseFloat(match[1]) : 0;
                const newY = Math.max(Math.min(currentY + yPos, 20), -20);

                word.style.transform = `translateY(${newY}px)`;
            });
        }

        kineticScrollY = scrollY;
    }

    let kineticTicking = false;
    window.addEventListener('scroll', () => {
        if (!kineticTicking) {
            window.requestAnimationFrame(() => {
                updateKineticTypography();
                kineticTicking = false;
            });
            kineticTicking = true;
        }
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
        // All users use the same install method now (git clone)
        const defaultTab = 'install';

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
