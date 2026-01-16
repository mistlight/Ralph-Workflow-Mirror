/**
 * Open Graph Image Generator for Ralph Workflow
 * Creates a 1200x630px social sharing image with Terminal Noir aesthetic
 */

import fs from 'fs';
import { createCanvas } from 'canvas';

// Dimensions
const WIDTH = 1200;
const HEIGHT = 630;

// Terminal Noir color palette
const colors = {
    background: '#0a0a0b',
    cyan: '#00d4ff',
    magenta: '#ff006e',
    lime: '#a3ff12',
    white: '#e4e4e7',
    gray: '#151518',
    border: '#27272a',
    muted: '#71717a'
};

function createOGImage() {
    const canvas = createCanvas(WIDTH, HEIGHT);
    const ctx = canvas.getContext('2d');

    // 1. Deep charcoal background
    ctx.fillStyle = colors.background;
    ctx.fillRect(0, 0, WIDTH, HEIGHT);

    // 2. Create gradient mesh effect with multiple radial gradients
    // Top-left cyan glow
    const gradient1 = ctx.createRadialGradient(200, 200, 0, 200, 200, 400);
    gradient1.addColorStop(0, 'rgba(0, 212, 255, 0.15)');
    gradient1.addColorStop(1, 'rgba(0, 212, 255, 0)');
    ctx.fillStyle = gradient1;
    ctx.fillRect(0, 0, WIDTH, HEIGHT);

    // Bottom-right magenta glow
    const gradient2 = ctx.createRadialGradient(1000, 430, 0, 1000, 430, 400);
    gradient2.addColorStop(0, 'rgba(255, 0, 110, 0.12)');
    gradient2.addColorStop(1, 'rgba(255, 0, 110, 0)');
    ctx.fillStyle = gradient2;
    ctx.fillRect(0, 0, WIDTH, HEIGHT);

    // Center lime accent
    const gradient3 = ctx.createRadialGradient(600, 315, 0, 600, 315, 300);
    gradient3.addColorStop(0, 'rgba(163, 255, 18, 0.08)');
    gradient3.addColorStop(1, 'rgba(163, 255, 18, 0)');
    ctx.fillStyle = gradient3;
    ctx.fillRect(0, 0, WIDTH, HEIGHT);

    // 3. Add grid pattern overlay
    ctx.strokeStyle = 'rgba(0, 212, 255, 0.03)';
    ctx.lineWidth = 1;
    const gridSize = 40;

    for (let x = 0; x < WIDTH; x += gridSize) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, HEIGHT);
        ctx.stroke();
    }

    for (let y = 0; y < HEIGHT; y += gridSize) {
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(WIDTH, y);
        ctx.stroke();
    }

    // 4. Draw editorial border frame
    ctx.strokeStyle = colors.cyan;
    ctx.lineWidth = 4;
    ctx.strokeRect(30, 30, WIDTH - 60, HEIGHT - 60);

    // Inner border
    ctx.strokeStyle = colors.magenta;
    ctx.lineWidth = 2;
    ctx.strokeRect(40, 40, WIDTH - 80, HEIGHT - 80);

    // 5. Draw "RALPH" title - editorial bold
    ctx.font = 'bold 180px "Arial Black", "Impact", sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';

    // Text shadow/glow effect
    ctx.shadowColor = colors.cyan;
    ctx.shadowBlur = 30;
    ctx.fillStyle = colors.cyan;
    ctx.fillText('RALPH', WIDTH / 2, HEIGHT / 2 - 40);

    // Reset shadow
    ctx.shadowBlur = 0;

    // 6. Draw subtitle
    ctx.font = 'bold 48px "Arial", sans-serif';
    ctx.fillStyle = colors.white;
    ctx.fillText('YOUR IDEAS, SHIPPED BY AI', WIDTH / 2, HEIGHT / 2 + 70);

    // 7. Draw tagline with magenta
    ctx.font = '32px "Arial", sans-serif';
    ctx.fillStyle = colors.magenta;
    ctx.fillText('FROM PROMPT.MD TO COMMITTED CODE', WIDTH / 2, HEIGHT / 2 + 130);

    // 8. Draw corner accents (editorial elements)
    // Top-left corner
    ctx.fillStyle = colors.cyan;
    ctx.fillRect(30, 30, 60, 6);
    ctx.fillRect(30, 30, 6, 60);

    // Top-right corner
    ctx.fillStyle = colors.magenta;
    ctx.fillRect(WIDTH - 90, 30, 60, 6);
    ctx.fillRect(WIDTH - 36, 30, 6, 60);

    // Bottom-left corner
    ctx.fillStyle = colors.magenta;
    ctx.fillRect(30, HEIGHT - 36, 60, 6);
    ctx.fillRect(30, HEIGHT - 90, 6, 60);

    // Bottom-right corner
    ctx.fillStyle = colors.lime;
    ctx.fillRect(WIDTH - 90, HEIGHT - 36, 60, 6);
    ctx.fillRect(WIDTH - 36, HEIGHT - 90, 6, 60);

    // 9. Draw terminal-style command at bottom
    ctx.font = '24px "Courier New", monospace';
    ctx.fillStyle = colors.lime;
    ctx.textAlign = 'left';
    ctx.fillText('$ cargo install ralph-workflow', 200, HEIGHT - 80);

    // 10. Add URL at bottom right
    ctx.font = 'bold 20px "Arial", sans-serif';
    ctx.fillStyle = colors.white;
    ctx.textAlign = 'right';
    ctx.fillText('codeberg.org/ralph', WIDTH - 200, HEIGHT - 80);

    // 11. Add decorative diagonal line (editorial element)
    ctx.strokeStyle = colors.magenta;
    ctx.lineWidth = 3;
    ctx.beginPath();
    ctx.moveTo(WIDTH - 250, 80);
    ctx.lineTo(WIDTH - 150, 180);
    ctx.stroke();

    // 12. Add small decorative dots
    const dotPositions = [
        { x: 150, y: 150, color: colors.cyan },
        { x: WIDTH - 180, y: 200, color: colors.magenta },
        { x: 180, y: HEIGHT - 200, color: colors.magenta },
        { x: WIDTH - 150, y: HEIGHT - 150, color: colors.lime }
    ];

    dotPositions.forEach(dot => {
        ctx.fillStyle = dot.color;
        ctx.beginPath();
        ctx.arc(dot.x, dot.y, 8, 0, Math.PI * 2);
        ctx.fill();
    });

    // Save image
    const buffer = canvas.toBuffer('image/png');
    fs.writeFileSync('og-image.png', buffer);

    console.log('Open Graph image created: og-image.png (1200x630px)');
    return canvas;
}

// Generate the image
if (import.meta.url === `file://${process.argv[1]}`) {
    try {
        createOGImage();
    } catch (error) {
        console.error('Error creating OG image:', error.message);
        console.log('Note: Make sure canvas package is installed: npm install canvas');
    }
}

export { createOGImage, colors };
