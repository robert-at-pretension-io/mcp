// Handles rendering and interactions for the chat/conversation area

import { escapeHtml, formatToolCalls } from '../utils/helpers.js';
import { emitUserMessage } from '../socket/socketClient.js';
import * as appState from '../state/appState.js'; // To check thinking state

// DOM Elements
let conversationElement;
let userInputElement;
let sendButton;
let clearButton;
let thinkingSpinner; // Use the spinner element

// Function to adjust textarea height based on content
function adjustTextareaHeight() {
    if (!userInputElement) return;
    
    // Reset height to auto to get accurate scrollHeight
    userInputElement.style.height = 'auto';
    
    // Set new height based on content (with min/max values)
    const newHeight = Math.max(Math.min(userInputElement.scrollHeight, 300), 40);
    userInputElement.style.height = `${newHeight}px`;
}

export function init() {
    conversationElement = document.getElementById('conversation');
    userInputElement = document.getElementById('user-input');
    sendButton = document.getElementById('send-button');
    clearButton = document.getElementById('clear-button');
    thinkingSpinner = document.getElementById('thinking-spinner'); // Get spinner

    if (!conversationElement || !userInputElement || !sendButton || !clearButton || !thinkingSpinner) {
        console.error("Chat UI elements not found!");
        return;
    }

    // Event Listeners
    sendButton.addEventListener('click', sendMessage);
    clearButton.addEventListener('click', handleClearConversation); // Use specific handler
    userInputElement.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            sendMessage();
        }
    });
    
    // Auto-expand textarea as user types
    userInputElement.addEventListener('input', adjustTextareaHeight);
    
    // Initial adjustment
    adjustTextareaHeight();

    console.log("Chat UI initialized.");
}

function sendMessage() {
    const message = userInputElement.value.trim();
    // Check state before sending
    if (message && !appState.isThinking()) {
        // Add user message optimistically
        addMessageToConversation('human', message); // Use 'human' role
        scrollToBottom(); // Scroll after adding user message

        // Send the message via socket
        emitUserMessage(message);

        // Clear the input field
        userInputElement.value = '';
        // Reset textarea height
        adjustTextareaHeight();

        // Update thinking indicator immediately (handled by socket 'thinking' event)
        // updateThinkingIndicator(true, 'Sending message...');
    } else if (appState.isThinking()) {
        console.log("Cannot send message while AI is thinking.");
        // Optionally show a toast or visual feedback
    }
}

function handleClearConversation() {
    // Confirmation could be added here if desired
    // if (confirm('Are you sure you want to clear the conversation?')) {
        // Emit clear event via socket
        emitClearConversation();
        // Toast notification is handled in sidebarUI where the button lives now?
        // Or handle it here based on socket 'conversation-cleared' event?
        // For now, assume socket handler triggers necessary UI updates/toasts.
    // }
}

export function renderConversationHistory(history) {
    if (!conversationElement) return;

    // Clear the conversation display
    conversationElement.innerHTML = '';

    // Render each message from the received history
    if (history && Array.isArray(history)) {
        history.forEach((message) => {
            // Skip system messages if they somehow get included
            if (message.role === 'system') return;

            addMessageToConversation(
                message.role,
                message.content,
                message.hasToolCalls,
                message.pendingToolCalls
            );
        });
    } else {
        console.warn("Received invalid history data:", history);
    }

    // Scroll to the bottom after rendering
    scrollToBottom();
}

function addMessageToConversation(role, content, hasToolCalls = false, isPending = false) {
    if (!conversationElement) return;

    // Create a wrapper div to better position messages
    const wrapperElement = document.createElement('div');
    wrapperElement.className = role === 'human' ? 'flex justify-end mb-3' : 'flex justify-start mb-3';
    
    const messageElement = document.createElement('div');
    // Map roles to CSS classes (e.g., 'human' -> 'user-message')
    const roleClassMap = {
        human: 'user-message',
        ai: 'ai-message',
        tool: 'tool-message',
        system: 'system-message', // Should generally be hidden
        error: 'error-message' // For displaying errors
    };
    const cssClass = roleClassMap[role] || 'system-message'; // Default to system/generic
    // Apply both traditional classes and Tailwind classes
    messageElement.className = `message ${cssClass} px-3 py-2 rounded-lg shadow-sm relative transition-all hover:translate-y-[-2px] hover:shadow-md`;

    if (isPending) {
        messageElement.classList.add('pending', 'opacity-70');
    }
    
    // Add styling based on role - maintain width but reduce vertical space
    if (role === 'human') {
        messageElement.classList.add('bg-blue-500', 'text-white', 'rounded-br-none', 'max-w-[85%]');
    } else if (role === 'ai') {
        messageElement.classList.add('bg-gray-100', 'dark:bg-gray-700', 'border', 'border-gray-200', 'dark:border-gray-600', 'rounded-bl-none', 'w-full', 'max-w-[85%]');
    } else if (role === 'system') {
        wrapperElement.className = 'flex justify-center mb-4';
        messageElement.classList.add('bg-amber-50', 'text-amber-800', 'border', 'border-amber-100', 'text-sm', 'max-w-[90%]');
    } else if (role === 'tool') {
        messageElement.classList.add('bg-gray-800', 'text-gray-100', 'font-mono', 'text-sm', 'whitespace-pre-wrap', 'w-full', 'py-2');
    } else if (role === 'error') {
        messageElement.classList.add('bg-red-50', 'text-red-800', 'border', 'border-red-200', 'w-full');
    }

    const now = new Date();
    const timeString = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }); // Simpler time format

    let roleLabel = role.charAt(0).toUpperCase() + role.slice(1);
    if (role === 'human') roleLabel = 'User';
    if (role === 'tool') roleLabel = 'Tool Result';
    if (role === 'error') roleLabel = 'Error'; // Use specific label for errors

    // Format content: escape HTML and format tool calls if necessary
    let contentHtml = '';
    if (role === 'ai' && hasToolCalls) {
        contentHtml = formatToolCalls(content); // Use helper
    } else {
        contentHtml = escapeHtml(content); // Use helper
    }

    messageElement.innerHTML = `
        <div class="message-header flex justify-between items-center mb-1 text-sm font-semibold">
            <span>${escapeHtml(roleLabel)}</span>
            <span class="message-time text-xs opacity-75">${timeString}</span>
        </div>
        <div class="message-content whitespace-pre-wrap break-words text-sm leading-normal">${contentHtml}</div>
    `;

    // Add message to wrapper, then add wrapper to conversation
    wrapperElement.appendChild(messageElement);
    conversationElement.appendChild(wrapperElement);
    // Don't scroll here, let renderConversationHistory handle it once at the end
}

// Function to display errors in the chat area
export function displayError(message) {
     addMessageToConversation('error', message);
     scrollToBottom(); // Scroll after adding error
}


// Function to clear the display (called on 'conversation-cleared' event)
export function clearConversationDisplay() {
    if (conversationElement) {
        conversationElement.innerHTML = '';
        // Optionally add a system message like "Conversation cleared."
        // addSystemMessage("Conversation cleared.");
    }
}

// Function to update the thinking indicator UI
export function updateThinkingIndicator(thinking, statusMessage = 'AI is thinking...') {
    if (!thinkingSpinner || !sendButton || !userInputElement) return;

    appState.setThinking(thinking); // Update shared state

    if (thinking) {
        thinkingSpinner.classList.remove('hidden');
        sendButton.disabled = true;
        userInputElement.disabled = true; // Disable input while thinking
        appState.setStatus(statusMessage); // Update footer status via state
    } else {
        thinkingSpinner.classList.add('hidden');
        sendButton.disabled = false;
        userInputElement.disabled = false;
        appState.setStatus('Ready'); // Reset footer status via state
    }
}

function scrollToBottom() {
    if (conversationElement) {
        conversationElement.scrollTop = conversationElement.scrollHeight;
        // Ensure the last message is fully visible
        const lastMessage = conversationElement.lastElementChild;
        if (lastMessage) {
            lastMessage.scrollIntoView({ behavior: 'smooth', block: 'end' });
        }
    }
}

// Helper to add system messages (if needed for UI feedback, though generally hidden)
function addSystemMessage(message) {
    addMessageToConversation('system', message);
    scrollToBottom();
}
