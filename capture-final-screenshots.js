// Wrapper script to capture final screenshots using the unified capture script
// Usage: node capture-final-screenshots.js
import { spawn } from 'child_process';

// Run the unified capture script with 'final' argument
const child = spawn('node', ['capture-screenshots.js', 'final'], {
    stdio: 'inherit',
    shell: true
});

child.on('error', (error) => {
    console.error(`Error: ${error.message}`);
    process.exit(1);
});

child.on('exit', (code) => {
    process.exit(code);
});
