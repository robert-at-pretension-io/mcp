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
const providersListElement = document.getElementById('providers-list');
const statusElement = document.getElementById('status');
const conversationsListElement = document.getElementById('conversations-list');
const newConversationBtn = document.getElementById('new-conversation-btn');

// Model modal elements
const modelModal = document.getElementById('model-modal');
const modelCloseBtn = modelModal.querySelector('.close');
const changeModelBtn = document.getElementById('change-model-btn');
const providerSelect = document.getElementById('provider-select');
const modelSelect = document.getElementById('model-select');
const apiKeyInput = document.getElementById('api-key-input');
const toggleApiKeyVisibilityBtn = document.getElementById('toggle-api-key-visibility');
const cancelModelChangeBtn = document.getElementById('cancel-model-change');
const applyModelChangeBtn = document.getElementById('apply-model-change');

// Servers modal elements
const serversModal = document.getElementById('servers-modal');
const serversCloseBtn = serversModal.querySelector('.close');
const manageServersBtn = document.getElementById('manage-servers-btn');
const serverListItems = document.getElementById('server-list-items');
const addServerBtn = document.getElementById('add-server-btn');
const serverForm = document.getElementById('server-form');
const noServerSelected = document.getElementById('no-server-selected');
const serverNameInput = document.getElementById('server-name');
const serverCommandInput = document.getElementById('server-command');
const serverArgsList = document.getElementById('server-args');
const serverEnvList = document.getElementById('server-env');
const addArgBtn = document.getElementById('add-arg-btn');
const addEnvBtn = document.getElementById('add-env-btn');
const cancelServersChangeBtn = document.getElementById('cancel-servers-change');
const applyServersChangeBtn = document.getElementById('apply-servers-change');

// Config editor modal elements
const configModal = document.getElementById('config-modal');
const configCloseBtn = configModal.querySelector('.close');
const configFileNameElement = document.getElementById('config-file-name');
const configEditor = document.getElementById('config-editor');
const cancelConfigChangeBtn = document.getElementById('cancel-config-change');
const applyConfigChangeBtn = document.getElementById('apply-config-change');
const editConfigsBtn = document.getElementById('edit-configs-btn');
const configOptionButtons = document.querySelectorAll('.config-option');

// Application state
let isThinking = false;
let currentProvider = '';
let providers = {};
let providerModels = {};
let serverConfig = { mcpServers: {} };
let selectedServerName = null;
let currentApiKey = '';
let toastTimeout = null;
let conversations = [];
let currentConversationId = null;
let currentConfigFile = null;

// Event Listeners
sendButton.addEventListener('click', sendMessage);
clearButton.addEventListener('click', clearConversation);
userInputElement.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        sendMessage();
    }
});

// Model modal event listeners
changeModelBtn.addEventListener('click', openModelModal);
modelCloseBtn.addEventListener('click', closeModelModal);
cancelModelChangeBtn.addEventListener('click', closeModelModal);
applyModelChangeBtn.addEventListener('click', applyModelChange);
providerSelect.addEventListener('change', updateModelSelectOptions);
toggleApiKeyVisibilityBtn.addEventListener('click', toggleApiKeyVisibility);

// Close model modal when clicking outside of it
window.addEventListener('click', (e) => {
    if (e.target === modelModal) {
        closeModelModal();
    }
});

// Server modal event listeners
manageServersBtn.addEventListener('click', openServersModal);
serversCloseBtn.addEventListener('click', closeServersModal);
cancelServersChangeBtn.addEventListener('click', closeServersModal);
applyServersChangeBtn.addEventListener('click', saveServerConfigurations);
addServerBtn.addEventListener('click', addNewServer);
addArgBtn.addEventListener('click', addArgument);
addEnvBtn.addEventListener('click', addEnvironmentVariable);

// Close servers modal when clicking outside of it
window.addEventListener('click', (e) => {
    if (e.target === serversModal) {
        closeServersModal();
    }
});

// Conversations event listeners
newConversationBtn.addEventListener('click', createNewConversation);

// Config editor event listeners
editConfigsBtn.addEventListener('click', showConfigOptions);
configCloseBtn.addEventListener('click', closeConfigModal);
cancelConfigChangeBtn.addEventListener('click', closeConfigModal);
applyConfigChangeBtn.addEventListener('click', saveConfigFile);

// Add click event to config option buttons
configOptionButtons.forEach(button => {
    button.addEventListener('click', () => {
        const fileName = button.dataset.file;
        openConfigEditor(fileName);
    });
});

// Close config modal when clicking outside of it
window.addEventListener('click', (e) => {
    if (e.target === configModal) {
        closeConfigModal();
    }
});

// Socket Event Handlers
socket.on('connect', () => {
    console.log('Connected to server');
    updateStatus('Connected');
    
    // Load providers data on connect
    fetchProviders();
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

socket.on('model-changed', (data) => {
    updateModelInfo(data.provider, data.model);
    closeModelModal();
    displayModelChangedMessage(data.provider, data.model);
});

socket.on('conversations-list', (data) => {
    conversations = data.conversations || [];
    renderConversationsList();
});

socket.on('conversation-loaded', (data) => {
    currentConversationId = data.id;
    updateConversationSelection();
    renderConversationHistory(data.messages);
});

socket.on('conversation-saved', (data) => {
    showToast('success', 'Success', 'Conversation saved');
    // Update conversation in list if exists
    const index = conversations.findIndex(c => c.id === data.id);
    if (index >= 0) {
        conversations[index] = data;
    } else {
        conversations.push(data);
    }
    currentConversationId = data.id;
    renderConversationsList();
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

// Initialize by loading conversation list on startup
function initializeApp() {
    fetchConversations();
}

// Fetch conversations list from the server
function fetchConversations() {
    fetch('/api/conversations')
        .then(response => {
            if (!response.ok) {
                throw new Error('Failed to fetch conversations');
            }
            return response.json();
        })
        .then(data => {
            conversations = data.conversations || [];
            renderConversationsList();
            
            // If no current conversation, and we have conversations, load the most recent one
            if (!currentConversationId && conversations.length > 0) {
                // Sort conversations by updatedAt, descending
                const sortedConversations = [...conversations].sort((a, b) => {
                    return new Date(b.updatedAt) - new Date(a.updatedAt);
                });
                
                // Load the most recent conversation
                loadConversation(sortedConversations[0].id);
            }
        })
        .catch(error => {
            console.error('Error fetching conversations:', error);
            showToast('error', 'Error', 'Failed to load conversations: ' + error.message);
        });
}

function clearConversation() {
    if (confirm('Are you sure you want to clear the conversation?')) {
        socket.emit('clear-conversation');
    }
}

function clearConversationDisplay() {
    conversationElement.innerHTML = '';
}

function renderConversationsList() {
    if (!conversations || conversations.length === 0) {
        conversationsListElement.innerHTML = '<div class="empty-list">No saved conversations</div>';
        return;
    }
    
    // Sort conversations by most recently updated
    const sortedConversations = [...conversations].sort((a, b) => {
        return new Date(b.updatedAt) - new Date(a.updatedAt);
    });
    
    let html = '';
    for (const conversation of sortedConversations) {
        const isActive = conversation.id === currentConversationId;
        const updatedDate = new Date(conversation.updatedAt);
        const formattedDate = updatedDate.toLocaleDateString() + ' ' + updatedDate.toLocaleTimeString();
        
        html += `
            <div class="conversation-item ${isActive ? 'active' : ''}" data-id="${escapeHtml(conversation.id)}">
                <div class="conversation-title">${escapeHtml(conversation.title || 'Untitled Conversation')}</div>
                <div class="conversation-meta">
                    <span class="conversation-model">${escapeHtml(conversation.provider || '')} - ${escapeHtml(conversation.modelName || '')}</span>
                    <span class="conversation-date" title="${formattedDate}">${formatRelativeTime(updatedDate)}</span>
                </div>
                <div class="conversation-actions">
                    <button class="conversation-rename-btn" title="Rename conversation"><i class="fas fa-edit"></i></button>
                    <button class="conversation-delete-btn" title="Delete conversation"><i class="fas fa-trash"></i></button>
                </div>
            </div>
        `;
    }
    
    conversationsListElement.innerHTML = html;
    
    // Add event listeners to conversation items
    document.querySelectorAll('.conversation-item').forEach(item => {
        // Load conversation when clicked
        item.addEventListener('click', (e) => {
            if (!e.target.closest('.conversation-actions')) {
                const conversationId = item.dataset.id;
                loadConversation(conversationId);
            }
        });
        
        // Rename conversation
        const renameBtn = item.querySelector('.conversation-rename-btn');
        if (renameBtn) {
            renameBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                const conversationId = item.dataset.id;
                const conversation = conversations.find(c => c.id === conversationId);
                if (conversation) {
                    const newTitle = prompt('Enter a new title for this conversation:', conversation.title || 'Untitled Conversation');
                    if (newTitle !== null) {
                        renameConversation(conversationId, newTitle.trim());
                    }
                }
            });
        }
        
        // Delete conversation
        const deleteBtn = item.querySelector('.conversation-delete-btn');
        if (deleteBtn) {
            deleteBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                const conversationId = item.dataset.id;
                if (confirm('Are you sure you want to delete this conversation? This cannot be undone.')) {
                    deleteConversation(conversationId);
                }
            });
        }
    });
}

function updateConversationSelection() {
    // Update the UI to highlight the currently selected conversation
    document.querySelectorAll('.conversation-item').forEach(item => {
        item.classList.toggle('active', item.dataset.id === currentConversationId);
    });
}

function createNewConversation() {
    // Request a new conversation from the server
    socket.emit('new-conversation');
    
    // Clear the conversation display in the UI
    clearConversationDisplay();
    
    // Reset currentConversationId
    currentConversationId = null;
    
    // Show a loading message
    addSystemMessage('Starting a new conversation...');
}

function loadConversation(conversationId) {
    if (!conversationId) return;
    
    // Show loading indicator
    updateStatus('Loading conversation...');
    
    // Request conversation load from server
    socket.emit('load-conversation', { id: conversationId });
}

function renameConversation(conversationId, newTitle) {
    if (!conversationId || !newTitle) return;
    
    fetch(`/api/conversations/${conversationId}/rename`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title: newTitle })
    })
    .then(response => {
        if (!response.ok) {
            return response.json().then(data => {
                throw new Error(data.error || 'Failed to rename conversation');
            });
        }
        return response.json();
    })
    .then(data => {
        // Update local conversation data
        const conversation = conversations.find(c => c.id === conversationId);
        if (conversation) {
            conversation.title = newTitle;
        }
        
        // Update UI
        renderConversationsList();
        showToast('success', 'Success', 'Conversation renamed');
    })
    .catch(error => {
        console.error('Error renaming conversation:', error);
        showToast('error', 'Error', 'Failed to rename conversation: ' + error.message);
    });
}

function deleteConversation(conversationId) {
    if (!conversationId) return;
    
    fetch(`/api/conversations/${conversationId}`, {
        method: 'DELETE'
    })
    .then(response => {
        if (!response.ok) {
            return response.json().then(data => {
                throw new Error(data.error || 'Failed to delete conversation');
            });
        }
        return response.json();
    })
    .then(data => {
        // Remove conversation from local list
        const index = conversations.findIndex(c => c.id === conversationId);
        if (index >= 0) {
            conversations.splice(index, 1);
        }
        
        // If the deleted conversation was the current one, create a new one
        if (currentConversationId === conversationId) {
            createNewConversation();
        }
        
        // Update UI
        renderConversationsList();
        showToast('success', 'Success', 'Conversation deleted');
    })
    .catch(error => {
        console.error('Error deleting conversation:', error);
        showToast('error', 'Error', 'Failed to delete conversation: ' + error.message);
    });
}

// Helper function to add a system message
function addSystemMessage(message) {
    const messageElement = document.createElement('div');
    messageElement.className = 'message system-message';
    
    const now = new Date();
    const timeString = now.toLocaleTimeString();
    
    messageElement.innerHTML = `
        <div class="message-header">
            <span>System</span>
            <span class="message-time">${timeString}</span>
        </div>
        <div class="message-content">${escapeHtml(message)}</div>
    `;
    
    conversationElement.appendChild(messageElement);
    conversationElement.scrollTop = conversationElement.scrollHeight;
}

// Format relative time (e.g. "2 hours ago")
function formatRelativeTime(date) {
    const now = new Date();
    const diffMs = now - date;
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHour = Math.floor(diffMin / 60);
    const diffDay = Math.floor(diffHour / 24);
    
    if (diffSec < 60) {
        return 'Just now';
    } else if (diffMin < 60) {
        return `${diffMin} min${diffMin !== 1 ? 's' : ''} ago`;
    } else if (diffHour < 24) {
        return `${diffHour} hour${diffHour !== 1 ? 's' : ''} ago`;
    } else if (diffDay < 7) {
        return `${diffDay} day${diffDay !== 1 ? 's' : ''} ago`;
    } else {
        return date.toLocaleDateString();
    }
}

// Configuration file editing functions
function showConfigOptions() {
    document.querySelectorAll('.config-option').forEach(btn => {
        btn.classList.add('highlight');
    });
    showToast('info', 'Info', 'Select a configuration file to edit');
    
    // Remove highlight after 3 seconds
    setTimeout(() => {
        document.querySelectorAll('.config-option').forEach(btn => {
            btn.classList.remove('highlight');
        });
    }, 3000);
}

function openConfigEditor(fileName) {
    if (!fileName) return;
    
    currentConfigFile = fileName;
    configFileNameElement.textContent = fileName;
    
    // Update status and show loading in editor
    updateStatus(`Loading ${fileName}...`);
    configEditor.value = 'Loading...';
    
    // Fetch the file content
    fetch(`/api/config/${fileName}`)
        .then(response => {
            if (!response.ok) {
                return response.json().then(data => {
                    throw new Error(data.error || `Failed to load ${fileName}`);
                });
            }
            return response.json();
        })
        .then(data => {
            // Format content nicely if it's JSON
            let formattedContent = data.content;
            if (fileName.endsWith('.json')) {
                try {
                    const jsonObj = JSON.parse(data.content);
                    formattedContent = JSON.stringify(jsonObj, null, 2);
                } catch (e) {
                    console.warn('Failed to parse and format JSON:', e);
                }
            }
            
            // Set the editor content
            configEditor.value = formattedContent;
            
            // Show the modal
            configModal.style.display = 'block';
            updateStatus('Ready');
        })
        .catch(error => {
            console.error(`Error loading ${fileName}:`, error);
            showToast('error', 'Error', `Failed to load ${fileName}: ${error.message}`);
            updateStatus('Ready');
            closeConfigModal();
        });
}

function closeConfigModal() {
    configModal.style.display = 'none';
    currentConfigFile = null;
}

function saveConfigFile() {
    if (!currentConfigFile) return;
    
    const content = configEditor.value.trim();
    if (!content) {
        showToast('error', 'Error', 'Configuration content cannot be empty');
        return;
    }
    
    // Validate JSON if it's a JSON file
    if (currentConfigFile.endsWith('.json')) {
        try {
            JSON.parse(content);
        } catch (error) {
            showToast('error', 'Error', `Invalid JSON: ${error.message}`);
            return;
        }
    }
    
    // Save the file
    updateStatus(`Saving ${currentConfigFile}...`);
    
    fetch(`/api/config/${currentConfigFile}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content })
    })
    .then(response => {
        if (!response.ok) {
            return response.json().then(data => {
                throw new Error(data.error || `Failed to save ${currentConfigFile}`);
            });
        }
        return response.json();
    })
    .then(data => {
        closeConfigModal();
        showToast('success', 'Success', `${currentConfigFile} saved successfully`);
        updateStatus('Ready');
        
        // If we updated servers.json, refresh the server configuration
        if (currentConfigFile === 'servers.json') {
            fetchServerConfig();
        }
    })
    .catch(error => {
        console.error(`Error saving ${currentConfigFile}:`, error);
        showToast('error', 'Error', `Failed to save ${currentConfigFile}: ${error.message}`);
        updateStatus('Ready');
    });
}

function updateServerInfo(servers) {
    connectedServersElement.textContent = servers.length > 0 
        ? `${servers.length} server${servers.length > 1 ? 's' : ''} connected` 
        : 'No servers connected';
    
    // Get the AI model info
    fetch('/api/model')
        .then(response => response.json())
        .then(data => {
            if (data && data.model) {
                aiModelElement.textContent = data.model;
            } else {
                aiModelElement.textContent = 'Unknown model';
            }
        })
        .catch(error => {
            console.error('Error fetching AI model info:', error);
            aiModelElement.textContent = 'Unknown model';
        });
}

function updateModelInfo(provider, model) {
    currentProvider = provider;
    aiModelElement.textContent = model;
    
    // Update the displayed providers in the sidebar
    renderProvidersList();
}

function renderProvidersList() {
    if (!providers || Object.keys(providers).length === 0) {
        providersListElement.innerHTML = '<p>No AI providers available</p>';
        return;
    }
    
    let html = '';
    for (const [name, config] of Object.entries(providers)) {
        const isActive = name === currentProvider;
        html += `
            <div class="provider-item ${isActive ? 'active' : ''}" data-provider="${escapeHtml(name)}">
                <div class="provider-name">
                    ${escapeHtml(name)}
                    ${isActive ? '<i class="fas fa-check-circle"></i>' : ''}
                </div>
                <div class="provider-model">${escapeHtml(config.model || 'No model specified')}</div>
            </div>
        `;
    }
    
    providersListElement.innerHTML = html;
    
    // Add click event to provider items
    document.querySelectorAll('.provider-item').forEach(item => {
        item.addEventListener('click', () => {
            const providerName = item.dataset.provider;
            switchProvider(providerName);
        });
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

function displayModelChangedMessage(provider, model) {
    const messageElement = document.createElement('div');
    messageElement.className = 'message system-message';
    
    const now = new Date();
    const timeString = now.toLocaleTimeString();
    
    messageElement.innerHTML = `
        <div class="message-header">
            <span>System</span>
            <span class="message-time">${timeString}</span>
        </div>
        <div class="message-content">
            <i class="fas fa-exchange-alt"></i> Model changed to: ${escapeHtml(model)} (${escapeHtml(provider)})
        </div>
    `;
    
    conversationElement.appendChild(messageElement);
    conversationElement.scrollTop = conversationElement.scrollHeight;
}

// Modal functions for Model/API Key
function openModelModal() {
    // Fetch the latest provider data before opening
    fetchProviders().then(() => {
        populateProviderSelect();
        // Clear API key field
        apiKeyInput.value = '';
        currentApiKey = '';
        // Show the modal
        modelModal.style.display = 'block';
    });
}

function closeModelModal() {
    modelModal.style.display = 'none';
    apiKeyInput.value = ''; // Clear API key for security
}

function populateProviderSelect() {
    providerSelect.innerHTML = '';
    
    // Add provider options
    for (const [name, config] of Object.entries(providers)) {
        const option = document.createElement('option');
        option.value = name;
        option.textContent = name;
        if (name === currentProvider) {
            option.selected = true;
        }
        providerSelect.appendChild(option);
    }
    
    // Update model options for the selected provider
    updateModelSelectOptions();
}

function updateModelSelectOptions() {
    const selectedProvider = providerSelect.value;
    const providerKey = selectedProvider.toLowerCase();
    modelSelect.innerHTML = '';
    
    // Get available models for the selected provider
    const models = providerModels[providerKey]?.models || [];
    
    if (models.length === 0) {
        const option = document.createElement('option');
        option.value = '';
        option.textContent = 'No models available';
        modelSelect.appendChild(option);
        return;
    }
    
    // Add model options
    models.forEach(model => {
        const option = document.createElement('option');
        option.value = model;
        option.textContent = model;
        
        // Check if this model is the currently selected one
        if (providers[selectedProvider] && providers[selectedProvider].model === model) {
            option.selected = true;
        }
        
        modelSelect.appendChild(option);
    });
}

function toggleApiKeyVisibility() {
    if (apiKeyInput.type === 'password') {
        apiKeyInput.type = 'text';
        toggleApiKeyVisibilityBtn.innerHTML = '<i class="fas fa-eye-slash"></i>';
    } else {
        apiKeyInput.type = 'password';
        toggleApiKeyVisibilityBtn.innerHTML = '<i class="fas fa-eye"></i>';
    }
}

function applyModelChange() {
    const provider = providerSelect.value;
    const model = modelSelect.value;
    const apiKey = apiKeyInput.value.trim();
    
    if (!provider || !model) {
        showToast('error', 'Error', 'Please select both a provider and a model');
        return;
    }
    
    // Show loading indication
    updateStatus('Applying changes...');
    
    // Update API key if provided
    if (apiKey) {
        updateApiKey(provider, apiKey)
            .then(() => {
                // After API key is updated successfully, switch model if needed
                if (provider !== currentProvider) {
                    switchProvider(provider);
                } else if (providers[provider]?.model !== model) {
                    // Only model changed
                    switchModel(provider, model);
                } else {
                    // Nothing changed except API key, which is already updated
                    closeModelModal();
                    showToast('success', 'Success', 'API key updated successfully');
                }
            })
            .catch(error => {
                console.error('Error updating API key:', error);
                showToast('error', 'Error', 'Failed to update API key: ' + error.message);
                updateStatus('Ready');
            });
    } else {
        // No API key provided, just switch model if needed
        if (provider !== currentProvider) {
            switchProvider(provider);
        } else if (providers[provider]?.model !== model) {
            // Only model changed
            switchModel(provider, model);
        } else {
            // Nothing changed
            closeModelModal();
            showToast('info', 'Info', 'No changes to apply');
        }
    }
}

function updateApiKey(provider, apiKey) {
    return fetch('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ provider, apiKey })
    })
    .then(response => {
        if (!response.ok) {
            return response.json().then(data => {
                throw new Error(data.error || 'Failed to update API key');
            });
        }
        return response.json();
    });
}

function switchProvider(providerName) {
    fetch('/api/provider', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ provider: providerName })
    })
    .then(response => {
        if (!response.ok) {
            return response.json().then(data => {
                throw new Error(data.error || 'Failed to switch provider');
            });
        }
        return response.json();
    })
    .then(data => {
        console.log('Provider switched successfully:', data);
        // The socket event handler will update the UI
    })
    .catch(error => {
        console.error('Error switching provider:', error);
        displayError(`Failed to switch provider: ${error.message}`);
        updateStatus('Ready');
    });
}

function switchModel(provider, model) {
    fetch('/api/model', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ provider, model })
    })
    .then(response => {
        if (!response.ok) {
            return response.json().then(data => {
                throw new Error(data.error || 'Failed to switch model');
            });
        }
        return response.json();
    })
    .then(data => {
        console.log('Model switched successfully:', data);
        // The socket event handler will update the UI
    })
    .catch(error => {
        console.error('Error switching model:', error);
        displayError(`Failed to switch model: ${error.message}`);
        updateStatus('Ready');
    });
}

function fetchProviders() {
    return fetch('/api/providers')
        .then(response => {
            if (!response.ok) {
                throw new Error('Failed to fetch providers');
            }
            return response.json();
        })
        .then(data => {
            providers = data.providers;
            providerModels = data.models;
            currentProvider = data.current;
            
            // Render the providers list in the sidebar
            renderProvidersList();
            
            return data;
        })
        .catch(error => {
            console.error('Error fetching providers:', error);
            providersListElement.innerHTML = `<p>Error loading providers: ${error.message}</p>`;
        });
}

// Modal functions for Server Configuration
function openServersModal() {
    // Fetch the latest server config before opening
    fetchServerConfig().then(() => {
        renderServerList();
        serversModal.style.display = 'block';
    });
}

function closeServersModal() {
    serversModal.style.display = 'none';
    selectedServerName = null;
    hideServerForm();
}

function fetchServerConfig() {
    return fetch('/api/servers')
        .then(response => {
            if (!response.ok) {
                throw new Error('Failed to fetch server configuration');
            }
            return response.json();
        })
        .then(data => {
            serverConfig = data;
            return data;
        })
        .catch(error => {
            console.error('Error fetching server configuration:', error);
            showToast('error', 'Error', 'Failed to load server configuration: ' + error.message);
        });
}

function renderServerList() {
    serverListItems.innerHTML = '';
    
    if (!serverConfig || !serverConfig.mcpServers) {
        return;
    }
    
    const servers = Object.keys(serverConfig.mcpServers);
    
    if (servers.length === 0) {
        const message = document.createElement('li');
        message.textContent = 'No servers configured';
        message.className = 'server-list-item empty';
        serverListItems.appendChild(message);
        return;
    }
    
    servers.forEach(serverName => {
        const item = document.createElement('li');
        item.className = `server-list-item ${selectedServerName === serverName ? 'active' : ''}`;
        item.dataset.serverName = serverName;
        
        const nameSpan = document.createElement('span');
        nameSpan.textContent = serverName;
        
        const deleteBtn = document.createElement('button');
        deleteBtn.className = 'server-delete-btn';
        deleteBtn.innerHTML = '<i class="fas fa-trash"></i>';
        deleteBtn.title = 'Delete server';
        
        item.appendChild(nameSpan);
        item.appendChild(deleteBtn);
        
        // Add click event to select server
        item.addEventListener('click', (e) => {
            if (!e.target.closest('.server-delete-btn')) {
                selectServer(serverName);
            }
        });
        
        // Add click event to delete button
        deleteBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            if (confirm(`Are you sure you want to delete server "${serverName}"?`)) {
                deleteServer(serverName);
            }
        });
        
        serverListItems.appendChild(item);
    });
}

function selectServer(serverName) {
    // Update selected server
    selectedServerName = serverName;
    
    // Update UI
    document.querySelectorAll('.server-list-item').forEach(item => {
        item.classList.toggle('active', item.dataset.serverName === serverName);
    });
    
    // Show form and populate with server config
    showServerForm();
    
    // Populate form with server data
    const serverData = serverConfig.mcpServers[serverName];
    if (serverData) {
        // Set server name and command
        serverNameInput.value = serverName;
        serverCommandInput.value = serverData.command || '';
        
        // Populate arguments
        serverArgsList.innerHTML = '';
        if (serverData.args && Array.isArray(serverData.args)) {
            serverData.args.forEach(arg => {
                addArgItem(arg);
            });
        }
        
        // Populate environment variables
        serverEnvList.innerHTML = '';
        if (serverData.env && typeof serverData.env === 'object') {
            Object.entries(serverData.env).forEach(([key, value]) => {
                addEnvItem(key, value);
            });
        }
    }
}

function showServerForm() {
    serverForm.classList.remove('hidden');
    noServerSelected.classList.add('hidden');
}

function hideServerForm() {
    serverForm.classList.add('hidden');
    noServerSelected.classList.remove('hidden');
    
    // Clear form
    serverNameInput.value = '';
    serverCommandInput.value = '';
    serverArgsList.innerHTML = '';
    serverEnvList.innerHTML = '';
}

function addNewServer() {
    // Generate a unique name
    let newServerName = 'new-server';
    let counter = 1;
    
    while (serverConfig.mcpServers[newServerName]) {
        newServerName = `new-server-${counter}`;
        counter++;
    }
    
    // Create a basic server configuration
    serverConfig.mcpServers[newServerName] = {
        command: 'npx',
        args: [],
        env: {}
    };
    
    // Refresh the server list
    renderServerList();
    
    // Select the new server
    selectServer(newServerName);
}

function deleteServer(serverName) {
    if (serverConfig.mcpServers[serverName]) {
        delete serverConfig.mcpServers[serverName];
        
        // Refresh the server list
        renderServerList();
        
        // If the deleted server was selected, hide the form
        if (selectedServerName === serverName) {
            selectedServerName = null;
            hideServerForm();
        }
    }
}

function addArgument() {
    addArgItem('');
}

function addArgItem(value = '') {
    const argItem = document.createElement('div');
    argItem.className = 'arg-item';
    
    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'arg-input';
    input.value = value;
    input.placeholder = 'Argument value';
    
    const removeBtn = document.createElement('button');
    removeBtn.className = 'remove-btn';
    removeBtn.innerHTML = '<i class="fas fa-times"></i>';
    removeBtn.title = 'Remove argument';
    
    removeBtn.addEventListener('click', () => {
        argItem.remove();
    });
    
    argItem.appendChild(input);
    argItem.appendChild(removeBtn);
    
    serverArgsList.appendChild(argItem);
}

function addEnvironmentVariable() {
    addEnvItem('', '');
}

function addEnvItem(key = '', value = '') {
    const envItem = document.createElement('div');
    envItem.className = 'env-item';
    
    const keyInput = document.createElement('input');
    keyInput.type = 'text';
    keyInput.className = 'key-input';
    keyInput.value = key;
    keyInput.placeholder = 'Key';
    
    const valueInput = document.createElement('input');
    valueInput.type = 'text';
    valueInput.className = 'value-input';
    valueInput.value = value;
    valueInput.placeholder = 'Value';
    
    const removeBtn = document.createElement('button');
    removeBtn.className = 'remove-btn';
    removeBtn.innerHTML = '<i class="fas fa-times"></i>';
    removeBtn.title = 'Remove variable';
    
    removeBtn.addEventListener('click', () => {
        envItem.remove();
    });
    
    envItem.appendChild(keyInput);
    envItem.appendChild(valueInput);
    envItem.appendChild(removeBtn);
    
    serverEnvList.appendChild(envItem);
}

function saveServerConfigurations() {
    // If a server is selected, update its configuration first
    if (selectedServerName) {
        updateSelectedServerConfig();
    }
    
    // Send updated configuration to the server
    fetch('/api/servers', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ config: serverConfig })
    })
    .then(response => {
        if (!response.ok) {
            return response.json().then(data => {
                throw new Error(data.error || 'Failed to save server configuration');
            });
        }
        return response.json();
    })
    .then(data => {
        console.log('Server configuration saved:', data);
        closeServersModal();
        showToast('success', 'Success', data.message || 'Server configuration saved successfully');
    })
    .catch(error => {
        console.error('Error saving server configuration:', error);
        showToast('error', 'Error', 'Failed to save server configuration: ' + error.message);
    });
}

function updateSelectedServerConfig() {
    // Get values from form
    const oldServerName = selectedServerName;
    const newServerName = serverNameInput.value.trim();
    const command = serverCommandInput.value.trim();
    
    // Validate inputs
    if (!newServerName || !command) {
        showToast('error', 'Error', 'Server name and command are required');
        return false;
    }
    
    // Collect arguments
    const args = [];
    document.querySelectorAll('.arg-item .arg-input').forEach(input => {
        const value = input.value.trim();
        if (value) {
            args.push(value);
        }
    });
    
    // Collect environment variables
    const env = {};
    document.querySelectorAll('.env-item').forEach(item => {
        const keyInput = item.querySelector('.key-input');
        const valueInput = item.querySelector('.value-input');
        
        if (keyInput && valueInput) {
            const key = keyInput.value.trim();
            const value = valueInput.value.trim();
            
            if (key) {
                env[key] = value;
            }
        }
    });
    
    // Create server config
    const serverData = {
        command,
        args,
        env
    };
    
    // Handle server rename
    if (oldServerName !== newServerName) {
        // Remove old entry
        delete serverConfig.mcpServers[oldServerName];
        
        // Update selected server name
        selectedServerName = newServerName;
    }
    
    // Update config
    serverConfig.mcpServers[newServerName] = serverData;
    
    return true;
}

// Toast notification function
function showToast(type, title, message) {
    // Remove existing toast if any
    const existingToast = document.querySelector('.toast');
    if (existingToast) {
        existingToast.remove();
    }
    
    // Clear existing timeout
    if (toastTimeout) {
        clearTimeout(toastTimeout);
    }
    
    // Create toast element
    const toast = document.createElement('div');
    toast.className = `toast ${type}`;
    
    // Set icon based on type
    let icon = '';
    switch (type) {
        case 'success':
            icon = '<i class="fas fa-check-circle"></i>';
            break;
        case 'error':
            icon = '<i class="fas fa-exclamation-circle"></i>';
            break;
        case 'warning':
            icon = '<i class="fas fa-exclamation-triangle"></i>';
            break;
        case 'info':
            icon = '<i class="fas fa-info-circle"></i>';
            break;
        default:
            icon = '<i class="fas fa-bell"></i>';
    }
    
    // Create toast content
    toast.innerHTML = `
        <div class="toast-icon">${icon}</div>
        <div class="toast-content">
            <div class="toast-title">${escapeHtml(title)}</div>
            <div class="toast-message">${escapeHtml(message)}</div>
        </div>
        <button class="toast-close"><i class="fas fa-times"></i></button>
    `;
    
    // Add close functionality
    toast.querySelector('.toast-close').addEventListener('click', () => {
        toast.classList.remove('show');
        setTimeout(() => toast.remove(), 300);
    });
    
    // Add to document
    document.body.appendChild(toast);
    
    // Show the toast (wait a bit for the DOM to update)
    setTimeout(() => toast.classList.add('show'), 10);
    
    // Auto-hide after 4 seconds
    toastTimeout = setTimeout(() => {
        toast.classList.remove('show');
        setTimeout(() => toast.remove(), 300);
    }, 4000);
}

// Helper function to escape HTML
function escapeHtml(unsafe) {
    if (unsafe === undefined || unsafe === null) {
        return '';
    }
    if (typeof unsafe !== 'string') {
        return String(unsafe);
    }
    return unsafe
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}

// Initialize the application when the page is loaded
document.addEventListener('DOMContentLoaded', () => {
    initializeApp();
});