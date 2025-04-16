// Simple build script for when Tailwind CLI isn't available
import fs from 'fs';
import path from 'path';

// Paths
const baseCssPath = path.join(process.cwd(), 'web', 'css', 'base.css');
const outputPath = path.join(process.cwd(), 'web', 'build', 'tailwind.css');
const tailwindFallbackPath = path.join(process.cwd(), 'web', 'build', 'tailwind-fallback.css');

// Read base CSS
try {
  console.log('Building CSS...');
  
  // Check if tailwind fallback exists
  if (fs.existsSync(tailwindFallbackPath)) {
    const fallbackCss = fs.readFileSync(tailwindFallbackPath, 'utf8');
    const baseCss = fs.existsSync(baseCssPath) 
      ? fs.readFileSync(baseCssPath, 'utf8') 
      : '/* Base CSS not found */';
    
    // Combine files
    const combinedCss = `${baseCss}\n\n/* Fallback Tailwind utilities */\n${fallbackCss}`;
    
    // Write combined CSS
    fs.writeFileSync(outputPath, combinedCss);
    console.log('CSS built successfully using fallback approach.');
  } else {
    console.log('Tailwind fallback not found. Creating minimal version.');
    // If no fallback, just copy base CSS as is
    if (fs.existsSync(baseCssPath)) {
      const baseCss = fs.readFileSync(baseCssPath, 'utf8');
      fs.writeFileSync(outputPath, baseCss);
      console.log('Minimal CSS built. Consider installing Tailwind CSS for full functionality.');
    } else {
      console.error('Base CSS file not found!');
      process.exit(1);
    }
  }
} catch (error) {
  console.error('Error building CSS:', error);
  process.exit(1);
}