// Connect to the Socket.IO server
const socket = io();

// DOM Elements
const conversationElement = document.getElementById('conversation');
const userInputElement = document.getElementById('user-input');
const sendButton = document.getElementById('send-button');
const clearButton = document.getElementById('clear-button');
const connectedServersElement = document.getElementById('connected-servers');
const aiModelElement = document.getElementById('ai-model');
const toolsListElement = document.getElementById('tools-list');
const statusElement = document.getElementById('status');

// Application state
let isThinking = false;

// Event Listeners
sendButton.addEventListener('click', sendMessage);
clearButton.addEventListener('click', clearConversation);
userInputElement.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        sendMessage();
    }
});

// Socket Event Handlers
socket.on('connect', () => {
    console.log('Connected to server');
    updateStatus('Connected');
});

socket.on('disconnect', () => {
    console.log('Disconnected from server');
    updateStatus('Disconnected');
});

socket.on('servers-info', (data) => {
    updateServerInfo(data.servers);
});

socket.on('history', (data) => {
    renderConversationHistory(data.history);
});

socket.on('history-update', (data) => {
    renderConversationHistory(data.history);
});

socket.on('ai-response', (data) => {
    // The messages will be rendered from the history update
});

socket.on('thinking', (data) => {
    isThinking = data.status;
    updateThinkingIndicator();
});

socket.on('tools-info', (data) => {
    renderToolsList(data.tools);
});

socket.on('error', (data) => {
    displayError(data.message);
});

socket.on('conversation-cleared', () => {
    clearConversationDisplay();
});

// Functions
function sendMessage() {
    const message = userInputElement.value.trim();
    if (message && !isThinking) {
        // Add user message to the conversation
        addMessageToConversation('user', message);
        
        // Send the message to the server
        socket.emit('user-message', { message });
        
        // Clear the input field
        userInputElement.value = '';
        
        // Show thinking indicator
        isThinking = true;
        updateThinkingIndicator();
    }
}

function clearConversation() {
    if (confirm('Are you sure you want to clear the conversation?')) {
        socket.emit('clear-conversation');
    }
}

function clearConversationDisplay() {
    conversationElement.innerHTML = '';
}

function updateServerInfo(servers) {
    connectedServersElement.textContent = `Connected Servers: ${servers.join(', ') || 'None'}`;
    
    // Make an API request to get the AI model info
    fetch('/api/history')
        .then(response => response.json())
        .then(data => {
            const messages = data.history;
            // Find the AI client model if available
            if (messages && messages.length > 0) {
                const aiModelMessage = messages.find(msg => msg.content.includes('AI client model'));
                if (aiModelMessage) {
                    const modelMatch = aiModelMessage.content.match(/AI client model: (.*)/);
                    if (modelMatch && modelMatch[1]) {
                        aiModelElement.textContent = `AI Model: ${modelMatch[1]}`;
                        return;
                    }
                }
            }
            
            // If no AI model found in messages, make a direct request
            return fetch('/api/model');
        })
        .then(response => response.json())
        .then(data => {
            if (data && data.model) {
                aiModelElement.textContent = `AI Model: ${data.model}`;
            }
        })
        .catch(error => {
            console.error('Error fetching AI model info:', error);
            aiModelElement.textContent = 'AI Model: Unknown';
        });
}

function renderToolsList(tools) {
    if (!tools || tools.length === 0) {
        toolsListElement.innerHTML = '<p>No tools available</p>';
        return;
    }
    
    let html = '';
    tools.forEach(tool => {
        html += `
            <div class="tool-item">
                <h4>${escapeHtml(tool.name)}</h4>
                <div class="tool-description">${escapeHtml(tool.description || 'No description')}</div>
            </div>
        `;
    });
    
    toolsListElement.innerHTML = html;
}

function renderConversationHistory(history) {
    // Clear the conversation display
    conversationElement.innerHTML = '';
    
    // Render each message
    history.forEach((message) => {
        if (message.role === 'system') {
            // Don't display system messages in the UI
            return;
        }
        
        addMessageToConversation(
            message.role, 
            message.content, 
            message.hasToolCalls, 
            message.pendingToolCalls
        );
    });
    
    // Scroll to the bottom of the conversation
    conversationElement.scrollTop = conversationElement.scrollHeight;
}

function addMessageToConversation(role, content, hasToolCalls = false, isPending = false) {
    const messageElement = document.createElement('div');
    messageElement.className = `message ${role}-message`;
    if (isPending) {
        messageElement.classList.add('pending');
    }
    
    const now = new Date();
    const timeString = now.toLocaleTimeString();
    
    let roleLabel = role.charAt(0).toUpperCase() + role.slice(1);
    if (role === 'tool') {
        roleLabel = 'Tool Result';
    }
    
    let contentHtml = escapeHtml(content);
    
    // If this is an AI message with tool calls, format them
    if (role === 'ai' && hasToolCalls) {
        contentHtml = formatToolCalls(content);
    }
    
    messageElement.innerHTML = `
        <div class="message-header">
            <span>${roleLabel}</span>
            <span class="message-time">${timeString}</span>
        </div>
        <div class="message-content">${contentHtml}</div>
    `;
    
    conversationElement.appendChild(messageElement);
    conversationElement.scrollTop = conversationElement.scrollHeight;
}

function formatToolCalls(content) {
    // Find tool calls using regex (simplified - not a perfect parser)
    const toolCallRegex = /<<<TOOL_CALL>>>([\s\S]*?)<<<END_TOOL_CALL>>>/g;
    
    // Replace tool calls with formatted HTML
    return content.replace(toolCallRegex, (match, toolCallContent) => {
        // Try to parse the JSON part
        try {
            const jsonStart = toolCallContent.indexOf('{');
            const jsonEnd = toolCallContent.lastIndexOf('}') + 1;
            
            if (jsonStart >= 0 && jsonEnd > 0) {
                const jsonString = toolCallContent.substring(jsonStart, jsonEnd);
                const toolCall = JSON.parse(jsonString);
                
                // Format the tool call
                return `
                    <div class="tool-call">
                        <div class="tool-call-header">Tool Call: ${escapeHtml(toolCall.name)}</div>
                        <div class="tool-call-content">${escapeHtml(JSON.stringify(toolCall.arguments, null, 2))}</div>
                    </div>
                `;
            }
        } catch (error) {
            console.error('Error parsing tool call:', error);
        }
        
        // If parsing fails, just escape and return the original
        return `<pre>${escapeHtml(match)}</pre>`;
    });
}

function updateThinkingIndicator() {
    // Remove any existing thinking indicator
    const existingIndicator = document.querySelector('.thinking-indicator');
    if (existingIndicator) {
        existingIndicator.remove();
    }
    
    if (isThinking) {
        // Create and add the thinking indicator
        const indicator = document.createElement('div');
        indicator.className = 'thinking-indicator';
        indicator.innerHTML = `
            <div class="dot"></div>
            <div class="dot"></div>
            <div class="dot"></div>
        `;
        conversationElement.appendChild(indicator);
        
        // Scroll to the bottom
        conversationElement.scrollTop = conversationElement.scrollHeight;
        
        // Update status
        updateStatus('AI is thinking...');
    } else {
        updateStatus('Ready');
    }
}

function updateStatus(message) {
    statusElement.textContent = message;
}

function displayError(message) {
    const errorElement = document.createElement('div');
    errorElement.className = 'message error-message';
    
    const now = new Date();
    const timeString = now.toLocaleTimeString();
    
    errorElement.innerHTML = `
        <div class="message-header">
            <span>Error</span>
            <span class="message-time">${timeString}</span>
        </div>
        <div class="message-content">${escapeHtml(message)}</div>
    `;
    
    conversationElement.appendChild(errorElement);
    conversationElement.scrollTop = conversationElement.scrollHeight;
}

// Helper function to escape HTML
function escapeHtml(unsafe) {
    if (typeof unsafe !== 'string') {
        return '';
    }
    return unsafe
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}