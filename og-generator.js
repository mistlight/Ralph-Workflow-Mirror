/**
 * Open Graph Image Generator for Ralph Workflow
 * Creates a 1200x630px social sharing image with Forest Editorial aesthetic
 */

const fs = require('fs');
const { createCanvas } = require('canvas');

// Dimensions
const WIDTH = 1200;
const HEIGHT = 630;

// Forest Editorial color palette
const colors = {
    background: '#0d1f18',
    amber: '#d4a574',
    sage: '#7da58a',
    emerald: '#5d9e73',
    rose: '#c97878',
    white: '#f5f2ed',
    gray: '#1a3a2f',
    border: '#234f3d'
};

function createOGImage() {
    const canvas = createCanvas(WIDTH, HEIGHT);
    const ctx = canvas.getContext('2d');

    // 1. Deep forest background
    ctx.fillStyle = colors.background;
    ctx.fillRect(0, 0, WIDTH, HEIGHT);

    // 2. Create gradient mesh effect with multiple radial gradients
    // Top-left amber glow
    const gradient1 = ctx.createRadialGradient(200, 200, 0, 200, 200, 400);
    gradient1.addColorStop(0, 'rgba(212, 165, 116, 0.15)');
    gradient1.addColorStop(1, 'rgba(212, 165, 116, 0)');
    ctx.fillStyle = gradient1;
    ctx.fillRect(0, 0, WIDTH, HEIGHT);

    // Bottom-right sage glow
    const gradient2 = ctx.createRadialGradient(1000, 430, 0, 1000, 430, 400);
    gradient2.addColorStop(0, 'rgba(125, 165, 138, 0.12)');
    gradient2.addColorStop(1, 'rgba(125, 165, 138, 0)');
    ctx.fillStyle = gradient2;
    ctx.fillRect(0, 0, WIDTH, HEIGHT);

    // Center rose accent
    const gradient3 = ctx.createRadialGradient(600, 315, 0, 600, 315, 300);
    gradient3.addColorStop(0, 'rgba(201, 120, 120, 0.08)');
    gradient3.addColorStop(1, 'rgba(201, 120, 120, 0)');
    ctx.fillStyle = gradient3;
    ctx.fillRect(0, 0, WIDTH, HEIGHT);

    // 3. Add grid pattern overlay
    ctx.strokeStyle = 'rgba(245, 242, 237, 0.03)';
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
    ctx.strokeStyle = colors.amber;
    ctx.lineWidth = 4;
    ctx.strokeRect(30, 30, WIDTH - 60, HEIGHT - 60);

    // Inner border
    ctx.strokeStyle = colors.sage;
    ctx.lineWidth = 2;
    ctx.strokeRect(40, 40, WIDTH - 80, HEIGHT - 80);

    // 5. Draw "RALPH" title - editorial bold
    ctx.font = 'bold 180px "Arial Black", "Impact", sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';

    // Text shadow/glow effect
    ctx.shadowColor = colors.amber;
    ctx.shadowBlur = 30;
    ctx.fillStyle = colors.amber;
    ctx.fillText('RALPH', WIDTH / 2, HEIGHT / 2 - 40);

    // Reset shadow
    ctx.shadowBlur = 0;

    // 6. Draw subtitle
    ctx.font = 'bold 48px "Arial", sans-serif';
    ctx.fillStyle = colors.white;
    // Note: Canvas 2D doesn't support letterSpacing - simulate with wider spacing by drawing character by character
    // For simplicity, we'll use the standard fillText without letter spacing
    ctx.fillText('YOUR IDEAS, SHIPPED BY AI', WIDTH / 2, HEIGHT / 2 + 70);

    // 7. Draw tagline with sage
    ctx.font = '32px "Arial", sans-serif';
    ctx.fillStyle = colors.sage;
    ctx.fillText('FROM PROMPT.MD TO COMMITTED CODE', WIDTH / 2, HEIGHT / 2 + 130);

    // 8. Draw corner accents (editorial elements)
    // Top-left corner
    ctx.fillStyle = colors.amber;
    ctx.fillRect(30, 30, 60, 6);
    ctx.fillRect(30, 30, 6, 60);

    // Top-right corner
    ctx.fillStyle = colors.rose;
    ctx.fillRect(WIDTH - 90, 30, 60, 6);
    ctx.fillRect(WIDTH - 36, 30, 6, 60);

    // Bottom-left corner
    ctx.fillStyle = colors.sage;
    ctx.fillRect(30, HEIGHT - 36, 60, 6);
    ctx.fillRect(30, HEIGHT - 90, 6, 60);

    // Bottom-right corner
    ctx.fillStyle = colors.emerald;
    ctx.fillRect(WIDTH - 90, HEIGHT - 36, 60, 6);
    ctx.fillRect(WIDTH - 36, HEIGHT - 90, 6, 60);

    // 9. Draw terminal-style command at bottom
    ctx.font = '24px "Courier New", monospace';
    ctx.fillStyle = colors.gray;
    ctx.textAlign = 'left';
    ctx.fillText('$ cargo install ralph-workflow', 200, HEIGHT - 80);

    // 10. Add URL at bottom right
    ctx.font = 'bold 20px "Arial", sans-serif';
    ctx.fillStyle = colors.white;
    ctx.textAlign = 'right';
    ctx.fillText('codeberg.org/ralph', WIDTH - 200, HEIGHT - 80);

    // 11. Add decorative diagonal line (editorial element)
    ctx.strokeStyle = colors.rose;
    ctx.lineWidth = 3;
    ctx.beginPath();
    ctx.moveTo(WIDTH - 250, 80);
    ctx.lineTo(WIDTH - 150, 180);
    ctx.stroke();

    // 12. Add small decorative dots
    const dotPositions = [
        { x: 150, y: 150, color: colors.amber },
        { x: WIDTH - 180, y: 200, color: colors.sage },
        { x: 180, y: HEIGHT - 200, color: colors.rose },
        { x: WIDTH - 150, y: HEIGHT - 150, color: colors.emerald }
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
if (require.main === module) {
    try {
        createOGImage();
    } catch (error) {
        console.error('Error creating OG image:', error.message);
        console.log('Note: Make sure canvas package is installed: npm install canvas');
    }
}

module.exports = { createOGImage, colors };
