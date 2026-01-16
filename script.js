/**
 * Ralph Workflow - Enhanced JavaScript
 * Handles: mobile nav, install tabs, copy-to-clipboard, smooth scroll, scroll animations,
 * terminal typing effect, nav scroll detection, parallax effects, magnetic buttons,
 * cursor spotlight effect
 */

(function() {
    'use strict';

    // === Enhanced Magnetic Button Effect ===
    // Only apply to devices with fine pointer (mouse) for better performance on touch devices
    const hasFinePointer = window.matchMedia('(pointer: fine)').matches;
    const buttons = document.querySelectorAll('.btn');

    if (hasFinePointer) {
        buttons.forEach(btn => {
            btn.addEventListener('mousemove', function(e) {
                const rect = btn.getBoundingClientRect();
                const x = e.clientX - rect.left - rect.width / 2;
                const y = e.clientY - rect.top - rect.height / 2;

                // Enhanced magnetic effect with subtle scaling
                const moveX = x * 0.2;
                const moveY = y * 0.2;
                const distance = Math.sqrt(x * x + y * y);
                const scale = 1 + Math.min(distance * 0.001, 0.02); // Subtle scale up to 1.02

                btn.style.transform = `translate(${moveX}px, ${moveY}px) scale(${scale})`;
            });

            btn.addEventListener('mouseleave', function() {
                btn.style.transform = '';
            });
        });

        // === Extended Magnetic Effect for Cards ===
        // Apply subtle magnetic effect to feature cards, audience cards, and open source page cards
        const magneticElements = document.querySelectorAll('.feature-card, .audience-card, .card, .license-card, .contribute-card, .value-card');

        magneticElements.forEach(card => {
            card.addEventListener('mousemove', function(e) {
                const rect = card.getBoundingClientRect();
                const x = e.clientX - rect.left - rect.width / 2;
                const y = e.clientY - rect.top - rect.height / 2;

                // More subtle effect for cards - less movement, no scale
                const moveX = x * 0.03;
                const moveY = y * 0.03;

                card.style.transform = `translate(${moveX}px, ${moveY}px)`;
            });

            card.addEventListener('mouseleave', function() {
                card.style.transform = '';
            });
        });
    }

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
    // (Moved to Scroll Event Listener Cleanup section below)

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
    // (Registered in Scroll Event Listener Cleanup section below)

    // Initial call
    updateNav();

    // === Scroll-Triggered Animations (Intersection Observer) ===
    const observerOptions = {
        root: null,
        rootMargin: '0px 0px -100px 0px',
        threshold: 0.1
    };

    const scrollObserver = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                entry.target.classList.add('is-visible');
                // Optional: unobserve after revealing for one-time animation
                // scrollObserver.unobserve(entry.target);
            }
        });
    }, observerOptions);

    // Observe all scroll-reveal elements
    const scrollRevealElements = document.querySelectorAll(
        '.scroll-reveal, .scroll-reveal-from-left, .scroll-reveal-from-right, .scroll-reveal-scale, .scroll-reveal-group'
    );

    scrollRevealElements.forEach(el => {
        scrollObserver.observe(el);
    });

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

    // Terminal animation timing constants (in milliseconds)
    // Represents the delay for each terminal line to appear
    const TERMINAL_TIMING_FIRST_LINE = 600;      // First command appears
    const TERMINAL_TIMING_SECOND_LINE = 1200;    // Second command appears
    const TERMINAL_TIMING_THIRD_LINE = 1500;     // Third command appears
    const TERMINAL_TIMING_FOURTH_LINE = 1800;    // Fourth command appears
    const TERMINAL_TIMING_FIFTH_LINE = 2200;     // Fifth command appears
    const TERMINAL_TIMING_SIXTH_LINE = 2800;     // Sixth command appears
    const TERMINAL_TIMING_SEVENTH_LINE = 3500;   // Seventh command appears

    // Store original timeouts for speed control
    const originalTiming = [
        TERMINAL_TIMING_FIRST_LINE,
        TERMINAL_TIMING_SECOND_LINE,
        TERMINAL_TIMING_THIRD_LINE,
        TERMINAL_TIMING_FOURTH_LINE,
        TERMINAL_TIMING_FIFTH_LINE,
        TERMINAL_TIMING_SIXTH_LINE,
        TERMINAL_TIMING_SEVENTH_LINE
    ];

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
            // Validate speed value exists and is a valid number
            const rawSpeed = btn.dataset.speed;
            if (!rawSpeed) return;
            const speed = parseFloat(rawSpeed);
            if (isNaN(speed) || speed <= 0) return;
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
                        // Validate commandLine and querySelector result exist
                        if (commandLine) {
                            const commandElement = commandLine.querySelector('.terminal-command');
                            if (commandElement) {
                                commandElement.textContent = step.text;
                            }
                            commandLine.classList.add('typed');
                        }
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
            // Create SVG element safely using DOM API
            const svgNS = 'http://www.w3.org/2000/svg';
            const svg = document.createElementNS(svgNS, 'svg');
            svg.setAttribute('width', '14');
            svg.setAttribute('height', '14');
            svg.setAttribute('viewBox', '0 0 24 24');
            svg.setAttribute('fill', 'none');
            svg.setAttribute('stroke', 'currentColor');
            svg.setAttribute('stroke-width', '2');
            svg.style.marginRight = '6px';

            const circle = document.createElementNS(svgNS, 'circle');
            circle.setAttribute('cx', '12');
            circle.setAttribute('cy', '12');
            circle.setAttribute('r', '10');

            const polyline = document.createElementNS(svgNS, 'polyline');
            polyline.setAttribute('points', '12 6 12 12 16 14');

            svg.appendChild(circle);
            svg.appendChild(polyline);

            const textNode = document.createTextNode(' Running...');
            terminalRunDemo.textContent = '';
            terminalRunDemo.appendChild(svg);
            terminalRunDemo.appendChild(textNode);

            // Reset button after demo
            setTimeout(() => {
                terminalRunDemo.disabled = false;
                terminalRunDemo.style.opacity = '1';
                // Create SVG element safely using DOM API
                const svgNS2 = 'http://www.w3.org/2000/svg';
                const svg2 = document.createElementNS(svgNS2, 'svg');
                svg2.setAttribute('width', '14');
                svg2.setAttribute('height', '14');
                svg2.setAttribute('viewBox', '0 0 24 24');
                svg2.setAttribute('fill', 'none');
                svg2.setAttribute('stroke', 'currentColor');
                svg2.setAttribute('stroke-width', '2');
                svg2.style.marginRight = '6px';

                const polygon = document.createElementNS(svgNS2, 'polygon');
                polygon.setAttribute('points', '5 3 19 12 5 21 5 3');

                svg2.appendChild(polygon);

                const textNode2 = document.createTextNode('Run Full Demo');
                terminalRunDemo.textContent = '';
                terminalRunDemo.appendChild(svg2);
                terminalRunDemo.appendChild(textNode2);
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
            // Validate dataset.tab exists
            const targetTab = this.dataset.tab;
            if (!targetTab) return;

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

    // Check localStorage for saved preference with try-catch for private browsing
    let savedMode = 'simple';
    try {
        savedMode = localStorage.getItem('ralph-install-mode') || 'simple';
    } catch (e) {
        // localStorage unavailable (private browsing, storage disabled)
        // Show user-visible notification that settings won't persist
        showStorageWarning();
    }

    function showStorageWarning() {
        // Add a small, dismissible warning banner
        const warning = document.createElement('div');
        warning.setAttribute('role', 'alert');
        warning.style.cssText = 'position:fixed;bottom:20px;right:20px;background:#f59e0b;color:#000;padding:12px 16px;border-radius:8px;font-size:14px;font-weight:500;box-shadow:0 4px 12px rgba(0,0,0,0.15);z-index:10000;animation:slideUp 0.3s ease-out;max-width:300px;';

        const textSpan = document.createElement('span');
        textSpan.textContent = '⚠️ Settings won\'t save—storage disabled. ';
        warning.appendChild(textSpan);

        const closeButton = document.createElement('button');
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

            // Save preference with try-catch for private browsing
            try {
                localStorage.setItem('ralph-install-mode', newMode);
            } catch (e) {
                // Silently fail - preference just won't persist
                console.warn('Could not save install mode preference');
            }

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

    // Helper function to create SVG checkmark element safely
    function createCheckmarkSVG() {
        const svgNS = 'http://www.w3.org/2000/svg';
        const svg = document.createElementNS(svgNS, 'svg');
        svg.setAttribute('width', '16');
        svg.setAttribute('height', '16');
        svg.setAttribute('viewBox', '0 0 24 24');
        svg.setAttribute('fill', 'none');
        svg.setAttribute('stroke', 'currentColor');
        svg.setAttribute('stroke-width', '2');

        const polyline = document.createElementNS(svgNS, 'polyline');
        polyline.setAttribute('points', '20 6 9 17 4 12');
        svg.appendChild(polyline);

        return svg;
    }

    copyButtons.forEach(btn => {
        btn.addEventListener('click', async function() {
            const codeBlock = this.closest('.code-block');
            const codeElement = codeBlock?.querySelector('code');

            // Validate code element exists
            if (!codeElement) {
                console.warn('Copy button clicked but no code element found');
                return;
            }

            const code = codeElement.textContent;

            // Store original content to restore later
            const originalContent = this.cloneNode(true);

            let success = false;

            // Use Clipboard API (requires secure context)
            if (navigator.clipboard && window.isSecureContext) {
                try {
                    await navigator.clipboard.writeText(code);
                    success = true;
                } catch (err) {
                    // Silent fail - clipboard errors are not actionable for users
                }
            }

            if (success) {
                // Show success state with animation - use DOM API
                this.textContent = '';
                this.appendChild(createCheckmarkSVG());
                this.classList.add('copied');

                setTimeout(() => {
                    this.textContent = '';
                    this.appendChild(originalContent.cloneNode(true));
                    this.classList.remove('copied');
                }, 2000);
            } else {
                // Show error indication
                this.classList.add('copy-failed');
                setTimeout(() => {
                    this.classList.remove('copy-failed');
                }, 2000);
            }
        });
    });

    // === Smooth Scroll ===
    document.querySelectorAll('a[href^="#"]').forEach(anchor => {
        anchor.addEventListener('click', function(e) {
            const targetId = this.getAttribute('href');
            if (targetId === '#') return;

            // Validate targetId format and extract ID value
            if (!targetId || !targetId.startsWith('#') || targetId.length < 2) return;
            const idValue = targetId.substring(1);

            // Strict ID validation: only alphanumeric, underscore, hyphen, and must start with letter
            // This prevents injection attempts and ensures we only use valid HTML5 IDs
            if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(idValue)) return;

            // Use getElementById instead of querySelector for safer ID lookup
            // getElementById only accepts a string ID (not a selector) and is more secure
            const target = document.getElementById(idValue);
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
    const animationObserverOptions = {
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
    }, animationObserverOptions);

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

    // === Enhanced Scroll Animation Classes ===
    // Observe elements with new animation classes
    const enhancedAnimatedElements = document.querySelectorAll(
        '.fade-in-up, .fade-in-left, .fade-in-right'
    );

    const enhancedObserver = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                entry.target.classList.add('visible');
                enhancedObserver.unobserve(entry.target);
            }
        });
    }, {
        root: null,
        rootMargin: '0px 0px -50px 0px',
        threshold: 0.15
    });

    enhancedAnimatedElements.forEach(el => {
        enhancedObserver.observe(el);
    });

    // Cleanup function for observers (can be called if needed)
    function cleanupObservers() {
        animationObserver.disconnect();
        enhancedObserver.disconnect();
    }

    // Auto-cleanup on page unload to prevent memory leaks
    window.addEventListener('beforeunload', cleanupObservers);

    // === Scroll Event Listener Cleanup ===
    // Store scroll handler references for proper cleanup
    const scrollHandlers = [];

    function addScrollListener(handler) {
        scrollHandlers.push(handler);
        window.addEventListener('scroll', handler);
    }

    function cleanupScrollListeners() {
        scrollHandlers.forEach(handler => {
            window.removeEventListener('scroll', handler);
        });
        scrollHandlers.length = 0;
    }

    // Register scroll listeners that need cleanup
    // Parallax scroll handler
    let parallaxTicking = false;
    const parallaxHandler = () => {
        if (!parallaxTicking) {
            window.requestAnimationFrame(() => {
                updateParallax();
                parallaxTicking = false;
            });
            parallaxTicking = true;
        }
    };
    addScrollListener(parallaxHandler);

    // Nav scroll handler
    let navTicking = false;
    const navHandler = () => {
        if (!navTicking) {
            window.requestAnimationFrame(() => {
                updateNav();
                navTicking = false;
            });
            navTicking = true;
        }
    };
    addScrollListener(navHandler);

    // Register cleanup on page unload
    window.addEventListener('beforeunload', cleanupScrollListeners);

    // === Mousemove Listener Cleanup ===
    // The mousemove listener for cursor spotlight is throttled with requestAnimationFrame
    // and only processes when the spotlight is active. This is more efficient than repeatedly
    // adding/removing the listener. The listener will be automatically cleaned up on page unload.

    // === Character-Level Kinetic Typography ===
    // Wrap each character in hero words for individual animation
    const heroTitle = document.querySelector('.hero-title');

    function initCharacterTypography() {
        if (!heroTitle) return;

        const heroWords = document.querySelectorAll('.hero-word');

        heroWords.forEach((word, wordIndex) => {
            // Skip hero-title-accent - it uses background-clip: text for gradient effect
            // which doesn't work when text is wrapped in child spans
            if (word.classList.contains('hero-title-accent')) {
                // Just mark as complete after animation delay
                const totalDelay = (word.textContent.length * 0.05 + wordIndex * 0.3 + 0.3) * 1000;
                setTimeout(() => {
                    word.classList.add('word-complete');
                }, totalDelay + 600);
                return;
            }

            const text = word.textContent;
            const charCount = text.length;

            // Clear existing content - use textContent for simpler operation
            while (word.firstChild) {
                word.removeChild(word.firstChild);
            }

            // Wrap each character in a span
            [...text].forEach((char, charIndex) => {
                const span = document.createElement('span');
                span.textContent = char;
                span.className = 'hero-char';

                // Add character index for staggered animation
                span.style.setProperty('--char-index', charIndex);
                span.style.setProperty('--word-index', wordIndex);

                // Mark spaces and punctuation for special handling
                if (char === ' ') {
                    span.classList.add('space');
                } else if (['.', ',', '!', '?', '&', '-'].includes(char)) {
                    span.classList.add('punctuation');
                }

                word.appendChild(span);
            });

            // Mark word as complete after animation
            const totalDelay = (charCount * 0.05 + wordIndex * 0.3 + 0.3) * 1000;
            setTimeout(() => {
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
            // Enhanced hero title transformation on scroll
            if (heroTitle) {
                const scrollProgress = Math.min(scrollY / (window.innerHeight * 0.5), 1);
                const scaleValue = 1 - (scrollProgress * 0.15); // Subtle scale down
                const translateYValue = scrollProgress * -30; // Move up slightly

                heroTitle.style.transform = `scale(${scaleValue}) translateY(${translateYValue}px)`;

                // Add scrolling class for additional effects
                if (scrollProgress > 0.1) {
                    heroTitle.classList.add('scrolling');
                } else {
                    heroTitle.classList.remove('scrolling');
                }
            }

            // Individual word parallax
            heroWords.forEach((word, index) => {
                const speed = (index + 1) * 0.03; // Slightly more pronounced
                const yPos = scrollDelta * speed;
                const currentTransform = word.style.transform || 'translateY(0) scale(1)';
                const match = currentTransform.match(/translateY\(([^)]+)\)/);
                const currentY = match ? parseFloat(match[1]) : 0;
                const newY = Math.max(Math.min(currentY + yPos, 30), -30);

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
    // Note: This listener is intentionally long-lived as it needs to respond to system
    // preference changes throughout the page lifecycle. No cleanup needed.
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

    // === Cursor Spotlight Effect ===
    const cursorSpotlight = document.querySelector('.cursor-spotlight');
    const heroSection = document.querySelector('.hero');

    if (cursorSpotlight && heroSection && !prefersReducedMotion.matches) {
        let spotlightActive = false;

        // Activate spotlight when mouse enters hero
        heroSection.addEventListener('mouseenter', () => {
            spotlightActive = true;
            cursorSpotlight.classList.add('active');
        });

        heroSection.addEventListener('mouseleave', () => {
            spotlightActive = false;
            cursorSpotlight.classList.remove('active');
        });

        // Track mouse movement with throttling
        // Note: Event listener stays attached but only processes when spotlightActive is true
        // This is more efficient than adding/removing listeners repeatedly
        let spotlightTicking = false;
        document.addEventListener('mousemove', (e) => {
            if (!spotlightActive || !spotlightTicking) {
                requestAnimationFrame(() => {
                    if (spotlightActive) {
                        cursorSpotlight.style.left = e.clientX + 'px';
                        cursorSpotlight.style.top = e.clientY + 'px';
                    }
                    spotlightTicking = false;
                });
                spotlightTicking = true;
            }
        });
    }

    // === Dark Mode Toggle ===
    const darkModeToggle = document.getElementById('dark-mode-toggle');

    // Check for saved theme preference or system preference
    let savedTheme = null;
    try {
        savedTheme = localStorage.getItem('ralph-theme');
    } catch (e) {
        showStorageWarning();
    }

    // Function to set theme
    function setTheme(theme) {
        if (theme === 'light') {
            document.documentElement.setAttribute('data-theme', 'light');
        } else {
            document.documentElement.removeAttribute('data-theme');
        }
    }

    // Initialize theme
    if (savedTheme) {
        setTheme(savedTheme);
    } else if (window.matchMedia && window.matchMedia('(prefers-color-scheme: light)').matches) {
        setTheme('light');
    }
    // Default is dark (no attribute needed)

    // Dark mode toggle functionality
    if (darkModeToggle) {
        darkModeToggle.addEventListener('click', () => {
            const currentTheme = document.documentElement.getAttribute('data-theme');
            // null or absent means dark (default), 'light' means light mode
            const newTheme = currentTheme === 'light' ? 'dark' : 'light';

            setTheme(newTheme);

            // Save preference
            try {
                localStorage.setItem('ralph-theme', newTheme);
            } catch (e) {
                showStorageWarning();
            }

            // Add transition animation
            darkModeToggle.style.transform = 'rotate(180deg)';
            setTimeout(() => {
                darkModeToggle.style.transform = '';
            }, 300);
        });
    }

    // === Scroll Progress Indicator ===
    const scrollProgress = document.getElementById('scroll-progress');

    function updateScrollProgress() {
        if (!scrollProgress) return;

        const windowHeight = document.documentElement.scrollHeight - document.documentElement.clientHeight;
        const scrolled = (window.scrollY / windowHeight) * 100;
        const progress = Math.min(Math.max(scrolled, 0), 100);

        scrollProgress.style.width = `${progress}%`;
    }

    // Throttled scroll handler
    let scrollProgressTicking = false;
    window.addEventListener('scroll', () => {
        if (!scrollProgressTicking) {
            window.requestAnimationFrame(() => {
                updateScrollProgress();
                scrollProgressTicking = false;
            });
            scrollProgressTicking = true;
        }
    });

    // Initial call
    updateScrollProgress();

    // === Audience Selector ===
    const audienceOptions = document.querySelectorAll('.audience-option');
    const audienceSelector = document.getElementById('audience-selector');

    // Check localStorage for saved audience preference with try-catch for private browsing
    let savedAudience = null;
    try {
        savedAudience = localStorage.getItem('ralph-audience') || null;
    } catch (e) {
        showStorageWarning();
    }

    if (savedAudience && audienceSelector) {
        document.body.setAttribute('data-audience', savedAudience);
        audienceOptions.forEach(option => {
            if (option.dataset.audience === savedAudience) {
                option.setAttribute('aria-pressed', 'true');
            }
        });
    }

    audienceOptions.forEach(option => {
        option.addEventListener('click', () => {
            // Validate dataset.audience exists and is a valid value
            const audience = option.dataset.audience;
            const validAudiences = ['developer', 'vibe-coder', 'newcomer'];
            if (!audience || !validAudiences.includes(audience)) return;

            // Update button states
            audienceOptions.forEach(opt => {
                opt.setAttribute('aria-pressed', 'false');
            });
            option.setAttribute('aria-pressed', 'true');

            // Update body attribute for content filtering
            if (document.body.getAttribute('data-audience') === audience) {
                // Toggle off if clicking the same option
                document.body.removeAttribute('data-audience');
                try {
                    localStorage.removeItem('ralph-audience');
                } catch (e) {
                    showStorageWarning();
                }
            } else {
                document.body.setAttribute('data-audience', audience);
                try {
                    localStorage.setItem('ralph-audience', audience);
                } catch (e) {
                    showStorageWarning();
                }
            }

            // Smooth scroll to relevant section based on audience
            const sectionMap = {
                'developer': '#features',
                'vibe-coder': '#how-it-works',
                'newcomer': '#install'
            };

            // Validate audience is a valid key before accessing sectionMap (already validated above)
            if (!Object.hasOwn(sectionMap, audience)) return;

            const targetSection = sectionMap[audience];
            if (targetSection && document.body.getAttribute('data-audience')) {
                setTimeout(() => {
                    const target = document.querySelector(targetSection);
                    if (target) {
                        const navHeight = document.querySelector('.nav')?.offsetHeight || 0;
                        const targetPosition = target.offsetTop - navHeight - 20;

                        window.scrollTo({
                            top: targetPosition,
                            behavior: 'smooth'
                        });
                    }
                }, 300);
            }
        });
    });

    // === Interactive Demo ===
    const demoTabs = document.querySelectorAll('.demo-tab');
    const demoPanels = document.querySelectorAll('.demo-panel');
    const demoRunBtn = document.getElementById('demo-run-btn');
    const demoTerminal = document.getElementById('demo-terminal');
    const demoCode = document.getElementById('demo-code');
    const demoStatus = document.getElementById('demo-status');
    const demoIteration = document.getElementById('demo-iteration');
    const demoSteps = document.querySelectorAll('.demo-step');

    // Tab switching
    demoTabs.forEach(tab => {
        tab.addEventListener('click', () => {
            // Validate dataset.tab exists
            const targetTab = tab.dataset.tab;
            if (!targetTab) return;

            demoTabs.forEach(t => t.classList.remove('active'));
            tab.classList.add('active');

            demoPanels.forEach(panel => {
                if (panel.dataset.panel === targetTab) {
                    panel.classList.add('active');
                } else {
                    panel.classList.remove('active');
                }
            });
        });
    });

    // Demo simulation
    let demoRunning = false;
    let demoTimeouts = [];

    const demoWorkflow = [
        { step: 1, status: 'Developer agent analyzing...', delay: 1000, terminal: '<span class="demo-success">✓</span> Reading PROMPT.md\n<span class="demo-prompt">$</span> Developer agent: Analyzing requirements...' },
        { step: 1, status: 'Developer writing code...', delay: 2500, terminal: '<span class="demo-success">✓</span> Reading PROMPT.md\n<span class="demo-success">✓</span> Developer agent: Analyzing requirements...\n<span class="demo-prompt">$</span> Developer agent: Writing authentication module...' },
        { step: 2, status: 'Developer writing code...', delay: 4000, code: true },
        { step: 2, status: 'Reviewing code...', delay: 5500, terminal: '<span class="demo-success">✓</span> Reading PROMPT.md\n<span class="demo-success">✓</span> Developer agent: Analyzing requirements...\n<span class="demo-success">✓</span> Developer agent: Writing authentication module...\n<span class="demo-prompt">$</span> Reviewer agent: Checking code quality...' },
        { step: 3, status: 'Reviewer checking...', delay: 7000, terminal: '<span class="demo-success">✓</span> Reading PROMPT.md\n<span class="demo-success">✓</span> Developer agent: Analyzing requirements...\n<span class="demo-success">✓</span> Developer agent: Writing authentication module...\n<span class="demo-success">✓</span> Reviewer agent: Checking code quality...\n<span class="demo-warning">⚠</span> Found 2 issues: missing password validation, no rate limiting' },
        { step: 4, status: 'Fixing issues...', delay: 9000, terminal: '<span class="demo-success">✓</span> Reading PROMPT.md\n<span class="demo-success">✓</span> Developer agent: Analyzing requirements...\n<span class="demo-success">✓</span> Developer agent: Writing authentication module...\n<span class="demo-success">✓</span> Reviewer agent: Checking code quality...\n<span class="demo-warning">⚠</span> Found 2 issues: missing password validation, no rate limiting\n<span class="demo-prompt">$</span> Developer agent: Applying fixes...' },
        { step: 4, status: 'Re-reviewing...', delay: 11000, iteration: 2, terminal: '<span class="demo-success">✓</span> Reading PROMPT.md\n<span class="demo-success">✓</span> Developer agent: Analyzing requirements...\n<span class="demo-success">✓</span> Developer agent: Writing authentication module...\n<span class="demo-success">✓</span> Reviewer agent: Checking code quality...\n<span class="demo-warning">⚠</span> Found 2 issues: missing password validation, no rate limiting\n<span class="demo-success">✓</span> Developer agent: Applying fixes...\n<span class="demo-prompt">$</span> Reviewer agent: Re-checking code...' },
        { step: 5, status: 'Creating commit...', delay: 13000, terminal: '<span class="demo-success">✓</span> Reading PROMPT.md\n<span class="demo-success">✓</span> Developer agent: Analyzing requirements...\n<span class="demo-success">✓</span> Developer agent: Writing authentication module...\n<span class="demo-success">✓</span> Reviewer agent: Checking code quality...\n<span class="demo-warning">⚠</span> Found 2 issues: missing password validation, no rate limiting\n<span class="demo-success">✓</span> Developer agent: Applying fixes...\n<span class="demo-success">✓</span> Reviewer agent: Re-checking code...\n<span class="demo-success">✓</span> Code approved! Creating commit...' },
        { step: 5, status: 'Complete!', delay: 14500, terminal: '<span class="demo-success">✓</span> Reading PROMPT.md\n<span class="demo-success">✓</span> Developer agent: Analyzing requirements...\n<span class="demo-success">✓</span> Developer agent: Writing authentication module...\n<span class="demo-success">✓</span> Reviewer agent: Checking code quality...\n<span class="demo-warning">⚠</span> Found 2 issues: missing password validation, no rate limiting\n<span class="demo-success">✓</span> Developer agent: Applying fixes...\n<span class="demo-success">✓</span> Reviewer agent: Re-checking code...\n<span class="demo-success">✓</span> Code approved! Creating commit...\n<span class="demo-success">✓</span> Commit created: feat(auth): add user authentication with JWT sessions\n<span class="demo-time">Time: 2m 34s</span>' }
    ];

    const generatedCode = `// src/auth/mod.rs
use jwt::{decode, encode, Header, Validation};
use redis::AsyncCommands;

pub struct AuthService {
    redis: redis::Client,
    jwt_secret: String,
}

impl AuthService {
    pub async fn login(&self, email: &str, password: &str)
        -> Result<String, AuthError>
    {
        // Validate email format
        if !email.contains('@') {
            return Err(AuthError::InvalidEmail);
        }

        // Validate password strength (min 8 chars)
        if password.len() < 8 {
            return Err(AuthError::WeakPassword);
        }

        // Check credentials and generate JWT
        let token = self.generate_jwt(email)?;

        // Store session in Redis
        let mut conn = self.redis.get_async_connection().await?;
        conn.set_ex(format!("session:{}", email),
                   &token, 3600).await?;

        Ok(token)
    }

    pub async fn reset_password(&self, email: &str)
        -> Result<(), AuthError>
    {
        // Generate reset token
        let reset_token = self.generate_reset_token(email)?;

        // Send email with reset link
        self.send_reset_email(email, &reset_token).await?;

        Ok(())
    }
}`;

    function clearDemo() {
        demoTimeouts.forEach(t => clearTimeout(t));
        demoTimeouts = [];
        demoRunning = false;
        demoRunBtn.disabled = false;
        // Use DOM API instead of innerHTML
        demoRunBtn.textContent = '';
        const playIcon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        playIcon.setAttribute('width', '16');
        playIcon.setAttribute('height', '16');
        playIcon.setAttribute('viewBox', '0 0 24 24');
        playIcon.setAttribute('fill', 'none');
        playIcon.setAttribute('stroke', 'currentColor');
        playIcon.setAttribute('stroke-width', '2');
        const polygon = document.createElementNS('http://www.w3.org/2000/svg', 'polygon');
        polygon.setAttribute('points', '5 3 19 12 5 21 5 3');
        playIcon.appendChild(polygon);
        demoRunBtn.appendChild(playIcon);
        demoRunBtn.appendChild(document.createTextNode(' Run Demo'));
        demoSteps.forEach(step => step.classList.remove('active'));
    }

    function runDemo() {
        if (demoRunning) return;
        demoRunning = true;
        demoRunBtn.disabled = true;
        // Use DOM API instead of innerHTML
        demoRunBtn.textContent = '';
        const spinner = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        spinner.setAttribute('width', '16');
        spinner.setAttribute('height', '16');
        spinner.setAttribute('viewBox', '0 0 24 24');
        spinner.setAttribute('fill', 'none');
        spinner.setAttribute('stroke', 'currentColor');
        spinner.setAttribute('stroke-width', '2');
        spinner.style.animation = 'demo-spin 1s linear infinite';
        const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
        circle.setAttribute('cx', '12');
        circle.setAttribute('cy', '12');
        circle.setAttribute('r', '10');
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('d', 'M12 6v6l4 2');
        spinner.appendChild(circle);
        spinner.appendChild(path);
        demoRunBtn.appendChild(spinner);
        demoRunBtn.appendChild(document.createTextNode(' Running...'));

        demoWorkflow.forEach((action, index) => {
            const timeout = setTimeout(() => {
                // Update status
                if (demoStatus) demoStatus.textContent = action.status;

                // Update iteration counter
                if (action.iteration && demoIteration) {
                    demoIteration.textContent = action.iteration;
                }

                // Update terminal
                if (action.terminal && demoTerminal) {
                    // Use DOM API to construct terminal content safely
                    demoTerminal.textContent = '';
                    const lines = action.terminal.split('\n');
                    lines.forEach(lineText => {
                        const div = document.createElement('div');
                        div.className = 'demo-terminal-line';

                        // Parse known-safe span elements from the hardcoded demoWorkflow
                        // We manually create spans for the known markup patterns
                        let content = lineText;

                        // Create document fragment to hold the parsed content
                        const fragment = document.createDocumentFragment();

                        // Process known span patterns in the terminal line
                        let lastIndex = 0;
                        const spanPattern = /<span class="(demo-success|demo-warning|demo-prompt|demo-time)">([^<]*)<\/span>/g;
                        let match;

                        while ((match = spanPattern.exec(content)) !== null) {
                            // Add text before the span
                            if (match.index > lastIndex) {
                                fragment.appendChild(document.createTextNode(content.substring(lastIndex, match.index)));
                            }

                            // Create the span element
                            const span = document.createElement('span');
                            span.className = match[1];
                            span.textContent = match[2];
                            fragment.appendChild(span);

                            lastIndex = spanPattern.lastIndex;
                        }

                        // Add remaining text after the last span
                        if (lastIndex < content.length) {
                            fragment.appendChild(document.createTextNode(content.substring(lastIndex)));
                        }

                        div.appendChild(fragment);
                        demoTerminal.appendChild(div);
                    });
                }

                // Update code panel
                if (action.code && demoCode) {
                    // Use DOM API for safer content insertion
                    demoCode.textContent = '';
                    const codeLines = generatedCode.split('\n');
                    codeLines.forEach(lineText => {
                        const div = document.createElement('div');
                        div.className = 'demo-code-line';
                        div.textContent = lineText;
                        demoCode.appendChild(div);
                    });

                    // Auto-switch to code tab
                    const codeTab = document.querySelector('.demo-tab[data-tab="code"]');
                    if (codeTab) codeTab.click();
                } else if (action.terminal && index > 0) {
                    // Auto-switch to terminal tab
                    const terminalTab = document.querySelector('.demo-tab[data-tab="terminal"]');
                    if (terminalTab) terminalTab.click();
                }

                // Update active step - validate step is a number before using in selector
                demoSteps.forEach(step => step.classList.remove('active'));
                const stepNum = parseInt(action.step, 10);
                if (!isNaN(stepNum) && stepNum >= 1 && stepNum <= 5) {
                    const activeStep = document.querySelector(`.demo-step[data-step="${stepNum}"]`);
                    if (activeStep) activeStep.classList.add('active');
                }

                // Reset on completion - track this timeout for cleanup
                if (index === demoWorkflow.length - 1) {
                    const resetTimeout = setTimeout(() => {
                        clearDemo();
                    }, 3000);
                    demoTimeouts.push(resetTimeout);
                }
            }, action.delay);

            demoTimeouts.push(timeout);
        });
    }

    if (demoRunBtn) {
        demoRunBtn.addEventListener('click', () => {
            if (!demoRunning) {
                // Reset demo state - use DOM API for safer content insertion
                demoTerminal.textContent = '';
                const terminalLine = document.createElement('div');
                terminalLine.className = 'demo-terminal-line';
                const promptSpan = document.createElement('span');
                promptSpan.className = 'demo-prompt';
                promptSpan.textContent = '$ ';
                const commandText = document.createTextNode('ralph -S');
                terminalLine.appendChild(promptSpan);
                terminalLine.appendChild(commandText);
                demoTerminal.appendChild(terminalLine);

                demoCode.textContent = '';
                const codeLine = document.createElement('div');
                codeLine.className = 'demo-code-line';
                codeLine.textContent = '// Generated code will appear here...';
                demoCode.appendChild(codeLine);

                demoIteration.textContent = '0';
                demoSteps.forEach(step => step.classList.remove('active'));

                runDemo();
            }
        });
    }

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

    // Only auto-select on first visit - with try-catch for private browsing
    let tabSelected = false;
    try {
        tabSelected = localStorage.getItem('ralph-tab-selected') === 'true';
    } catch (e) {
        showStorageWarning();
    }

    if (!tabSelected) {
        detectPlatform();
        try {
            localStorage.setItem('ralph-tab-selected', 'true');
        } catch (e) {
            showStorageWarning();
        }
    }

    // === Expandable Feature Cards ===
    const expandButtons = document.querySelectorAll('.feature-expand-btn');

    expandButtons.forEach(btn => {
        btn.addEventListener('click', () => {
            // Validate dataset.expand exists
            const targetId = btn.dataset.expand;
            if (!targetId) return;
            const targetContent = document.getElementById(targetId);
            const isExpanded = btn.getAttribute('aria-expanded') === 'true';

            // Validate targetContent exists before manipulation
            if (!targetContent) {
                console.warn(`Expand button references missing element: ${targetId}`);
                return;
            }

            // Toggle expanded state
            btn.setAttribute('aria-expanded', !isExpanded);
            targetContent.setAttribute('aria-hidden', isExpanded);

            // Update button text
            const span = btn.querySelector('span');
            if (span) {
                span.textContent = isExpanded ? 'Learn more' : 'Show less';
            }
        });
    });

})();
