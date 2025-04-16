# CSS Architecture for MCP Multi-Client

## Overview

The CSS architecture for this project has been streamlined to reduce duplication and improve maintainability. All styles have been consolidated into a single source of truth.

## Structure

- **base.css**: Contains all styles for the application
  - CSS Custom Properties (variables)
  - Tailwind imports
  - Component styles
  - Animations
  - Responsive design

## Features

- **Theming**: Uses CSS custom properties for consistent theming
- **Components**: Reusable components defined with @apply directives
- **Dark Mode**: Consistent dark mode implementation using Tailwind's dark: prefix
- **Responsiveness**: Mobile-first approach with consistent breakpoints
- **Animations**: Smooth transitions and animations for better UX

## Usage Guidelines

1. Use Tailwind utility classes for one-off styling needs
2. Use component classes from base.css for repeated UI patterns
3. Respect the color variables for consistent theming
4. Maintain dark mode support for all new components
5. Keep responsive breakpoints consistent (768px, 1024px)

## Build Process

Tailwind processes the CSS and produces a single build/tailwind.css file that contains all styles needed for the application.