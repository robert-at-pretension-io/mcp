// Utility functions

/**
 * Escapes HTML special characters in a string.
 * @param {string | number | null | undefined} unsafe The string to escape.
 * @returns {string} The escaped string.
 */
export function escapeHtml(unsafe) {
    if (unsafe === undefined || unsafe === null) {
        return '';
    }
    if (typeof unsafe !== 'string') {
        // Attempt to convert non-strings before escaping
        try {
            unsafe = String(unsafe);
        } catch (e) {
            console.error("Failed to convert value to string for escaping:", unsafe, e);
            return ''; // Return empty string on conversion failure
        }
    }
    return unsafe
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}


/**
 * Formats a date object into a relative time string (e.g., "2 hours ago").
 * @param {Date} date The date object to format.
 * @returns {string} The relative time string.
 */
export function formatRelativeTime(date) {
    if (!(date instanceof Date) || isNaN(date.getTime())) {
        return 'Invalid date'; // Handle invalid date input
    }
    const now = new Date();
    const diffMs = now.getTime() - date.getTime(); // Use getTime() for reliable comparison

    // Handle future dates or very recent times
    if (diffMs < 0) return 'In the future';
    if (diffMs < 1000) return 'Just now'; // Less than a second

    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHour = Math.floor(diffMin / 60);
    const diffDay = Math.floor(diffHour / 24);
    const diffWeek = Math.floor(diffDay / 7);
    const diffMonth = Math.floor(diffDay / 30); // Approximate
    const diffYear = Math.floor(diffDay / 365); // Approximate

    if (diffSec < 60) {
        return `${diffSec} sec${diffSec !== 1 ? 's' : ''} ago`;
    } else if (diffMin < 60) {
        return `${diffMin} min${diffMin !== 1 ? 's' : ''} ago`;
    } else if (diffHour < 24) {
        return `${diffHour} hour${diffHour !== 1 ? 's' : ''} ago`;
    } else if (diffDay < 7) {
        return `${diffDay} day${diffDay !== 1 ? 's' : ''} ago`;
    } else if (diffWeek < 5) { // Show weeks up to about a month
        return `${diffWeek} week${diffWeek !== 1 ? 's' : ''} ago`;
    } else {
        // For older dates, show the actual date
        return date.toLocaleDateString(undefined, {
            year: 'numeric', month: 'short', day: 'numeric'
        });
    }
}

/**
 * Formats tool calls within AI response content.
 * @param {string} content The raw AI response content.
 * @returns {string} HTML formatted content with tool calls visualized.
 */
export function formatToolCalls(content) {
    // Basic regex, assumes JSON is well-formed between delimiters
    const toolCallRegex = /<<<TOOL_CALL>>>([\s\S]*?)<<<END_TOOL_CALL>>>/g;

    return content.replace(toolCallRegex, (match, toolCallContent) => {
        try {
            const toolCall = JSON.parse(toolCallContent.trim());
            const toolName = toolCall.name || 'unknown_tool';
            const argsString = JSON.stringify(toolCall.arguments || {}, null, 2);

            return `
                <div class="tool-call">
                    <div class="tool-call-header">Tool Call: ${escapeHtml(toolName)}</div>
                    <pre class="tool-call-content">${escapeHtml(argsString)}</pre>
                </div>
            `;
        } catch (error) {
            console.warn('Failed to parse tool call JSON:', error);
            // Fallback: display the raw content escaped
            return `<pre class="tool-call error">${escapeHtml(match)}</pre>`;
        }
    });
}
