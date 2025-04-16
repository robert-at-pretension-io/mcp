// Function to display toast notifications

import { escapeHtml } from '../utils/helpers.js';

let toastTimeout = null;

/**
 * Displays a toast notification.
 * @param {'success' | 'error' | 'warning' | 'info'} type The type of toast.
 * @param {string} title The title of the toast.
 * @param {string} message The main message content.
 */
export function showToast(type, title, message) {
    // Remove existing toast if any
    const existingToast = document.querySelector('.toast');
    if (existingToast) {
        // Allow fade-out animation to complete before removing
        existingToast.classList.remove('show');
        setTimeout(() => {
            if (existingToast.parentNode) {
                existingToast.remove();
            }
        }, 500); // Match transition duration
    }

    // Clear existing timeout
    if (toastTimeout) {
        clearTimeout(toastTimeout);
    }

    // Create toast element using Tailwind classes defined in base.css @layer components
    const toast = document.createElement('div');
    toast.className = `toast ${type}`; // Base class + type for styling

    // Set icon based on type using FontAwesome classes
    let iconClass = '';
    switch (type) {
        case 'success': iconClass = 'fas fa-check-circle'; break;
        case 'error': iconClass = 'fas fa-exclamation-circle'; break;
        case 'warning': iconClass = 'fas fa-exclamation-triangle'; break;
        case 'info': iconClass = 'fas fa-info-circle'; break;
        default: iconClass = 'fas fa-bell'; // Default icon
    }

    // Create toast content using Tailwind structure
    toast.innerHTML = `
        <div class="toast-icon"><i class="${iconClass}"></i></div>
        <div class="toast-content">
            <div class="toast-title">${escapeHtml(title)}</div>
            <div class="toast-message">${escapeHtml(message)}</div>
        </div>
        <button class="toast-close ml-auto p-1"><i class="fas fa-times"></i></button>
    `;

    // Add close functionality
    toast.querySelector('.toast-close').addEventListener('click', () => {
        toast.classList.remove('show');
        // Ensure removal even if transition fails or timeout cleared
        setTimeout(() => {
             if (toast.parentNode) toast.remove();
        }, 500);
        if (toastTimeout) clearTimeout(toastTimeout); // Clear timeout if closed manually
    });

    // Add to document
    document.body.appendChild(toast);

    // Show the toast (force reflow before adding class)
    requestAnimationFrame(() => {
        requestAnimationFrame(() => {
             toast.classList.add('show');
        });
    });


    // Auto-hide after 4 seconds
    toastTimeout = setTimeout(() => {
        toast.classList.remove('show');
        setTimeout(() => {
            if (toast.parentNode) toast.remove();
        }, 500); // Allow fade out
    }, 4000);
}
