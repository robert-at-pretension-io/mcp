import * as readline from 'node:readline';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
/**
 * Interactive REPL for interacting with MCP servers and AI agent
 */
// Helper to get the directory name in ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const baseDir = path.join(__dirname, '..');
export class Repl {
    rl;
    commands = new Map();
    serverManager;
    conversationManager; // Add ConversationManager instance
    currentServer = null; // For direct server interaction
    isChatMode = false; // Flag for chat mode
    running = false;
    providers = {}; // Available AI providers
    providerModels = {}; // Available models for providers
    currentProvider = ''; // Current provider name
    aiConfigPath = path.join(baseDir, 'ai_config.json'); // Path to AI config file
    // History handling might need adjustment for chat mode
    // private history: string[] = [];
    // private historyIndex = 0;
    constructor(serverManager, conversationManager, providerModels) {
        this.serverManager = serverManager;
        this.conversationManager = conversationManager; // Store ConversationManager
        this.providerModels = providerModels; // Store model list
        // Load AI config
        this.loadAiConfig();
        // Create readline interface
        this.rl = readline.createInterface({
            input: process.stdin,
            output: process.stdout,
            prompt: this.getPrompt(),
            historySize: 100,
        });
        // Register default commands
        this.registerCommands();
    }
    /**
     * Load the AI provider configuration from the config file
     */
    loadAiConfig() {
        try {
            const aiConfigFile = fs.readFileSync(this.aiConfigPath, 'utf-8');
            const aiConfigData = JSON.parse(aiConfigFile);
            // Store providers
            this.providers = aiConfigData.providers || {};
            // Store current provider
            this.currentProvider = aiConfigData.defaultProvider || '';
            console.log(`Loaded AI configurations for: ${Object.keys(this.providers).join(', ')}`);
            if (this.currentProvider) {
                console.log(`Current AI provider: ${this.currentProvider}`);
            }
        }
        catch (error) {
            console.error('Error loading AI config:', error instanceof Error ? error.message : String(error));
        }
    }
    /**
     * Save the AI provider configuration to the config file
     */
    saveAiConfig() {
        try {
            const aiConfigData = {
                defaultProvider: this.currentProvider,
                providers: this.providers
            };
            fs.writeFileSync(this.aiConfigPath, JSON.stringify(aiConfigData, null, 2), 'utf-8');
            console.log(`AI configuration saved to ${this.aiConfigPath}`);
        }
        catch (error) {
            console.error('Error saving AI config:', error instanceof Error ? error.message : String(error));
        }
    }
    getPrompt() {
        if (this.isChatMode) {
            const modelName = this.conversationManager.getAiClientModelName(); // Need method in ConversationManager
            return `Chat (${modelName})> `;
        }
        else {
            const serverText = this.currentServer || 'none';
            return `MCP [${serverText}]> `;
        }
    }
    registerCommands() {
        // --- Keep existing commands: help, servers, use, tools, call, exit, quit ---
        // Help command
        this.addCommand({
            name: 'help',
            description: 'Show available commands',
            execute: async () => {
                let output = 'Available commands:\n';
                output += '  chat         - Enter interactive chat mode with the AI agent.\n';
                output += '  exit         - Exit chat mode or the REPL.\n';
                output += '  quit         - Alias for exit.\n';
                output += '  --- AI/Model Commands ---\n';
                output += '  providers    - List available AI providers and models.\n';
                output += '  provider <name> - Set the current AI provider.\n';
                output += '  model <name> - Set the model for the current provider.\n';
                output += '  --- Server Commands ---\n';
                output += '  servers      - List all connected servers.\n';
                output += '  use <server> - Select a server for direct tool calls.\n';
                output += '  tools [srv]  - List tools on current or specified server.\n';
                output += '  call <tool> [json] - Call tool on current/auto-detected server.\n';
                output += '  --- Chat Commands ---\n';
                output += '  history      - Show conversation history (in chat mode).\n';
                output += '  clear        - Clear conversation history (in chat mode).\n';
                return output;
            }
        });
        // List servers command (no changes needed)
        this.addCommand({
            name: 'servers',
            description: 'List all connected servers',
            execute: async () => {
                const servers = this.serverManager.getConnectedServers();
                if (servers.length === 0) {
                    return 'No connected servers.';
                }
                let output = 'Connected servers:\n';
                servers.forEach(name => {
                    const isCurrent = name === this.currentServer;
                    output += `  ${isCurrent ? '* ' : '  '}${name}\n`;
                });
                return output;
            }
        });
        // Use server command (no changes needed)
        this.addCommand({
            name: 'use',
            description: 'Select a server to use for direct tool calls (use <server-name>)',
            execute: async (args) => {
                if (this.isChatMode)
                    return 'Cannot use specific servers in chat mode. Exit chat first.';
                if (!args[0]) {
                    return 'Error: Server name required.';
                }
                const serverName = args[0];
                const servers = this.serverManager.getConnectedServers();
                if (!servers.includes(serverName)) {
                    return `Error: Server '${serverName}' not found or not connected.`;
                }
                this.currentServer = serverName;
                this.rl.setPrompt(this.getPrompt());
                return `Now using server: ${serverName}`;
            }
        });
        // List tools command (no changes needed)
        this.addCommand({
            name: 'tools',
            description: 'List available tools on the current or specified server',
            execute: async (args) => {
                if (this.isChatMode)
                    return 'Use "chat" to interact with the AI which knows available tools. Exit chat for direct tool listing.';
                try {
                    const targetServer = args[0] || this.currentServer;
                    if (!targetServer) {
                        return 'Error: No server selected. Use "use <server-name>" or "tools <server-name>".';
                    }
                    const tools = this.serverManager.getServerTools(targetServer);
                    if (tools.length === 0) {
                        return `No tools available on server '${targetServer}'.`;
                    }
                    let output = `Tools available on server '${targetServer}':\n`;
                    tools.forEach(tool => {
                        // Add schema display later if needed
                        output += `  ${tool.name} - ${tool.description || 'No description'}\n`;
                    });
                    return output;
                }
                catch (error) {
                    return `Error: ${error instanceof Error ? error.message : String(error)}`;
                }
            }
        });
        // Execute tool command (no changes needed)
        this.addCommand({
            name: 'call',
            description: 'Execute a tool directly (call <tool-name> [json-args])',
            execute: async (args) => {
                if (this.isChatMode)
                    return 'Cannot call tools directly in chat mode. The AI will call tools if needed. Exit chat first.';
                if (!args[0]) {
                    return 'Error: Tool name required.';
                }
                const toolName = args[0];
                let toolArgs = {};
                // Parse JSON arguments if provided
                const jsonArgString = args.slice(1).join(' ');
                if (jsonArgString) {
                    try {
                        toolArgs = JSON.parse(jsonArgString);
                        if (typeof toolArgs !== 'object' || toolArgs === null || Array.isArray(toolArgs)) {
                            throw new Error("Arguments must be a JSON object.");
                        }
                    }
                    catch (error) {
                        return `Error parsing JSON arguments: ${error instanceof Error ? error.message : String(error)}`;
                    }
                }
                // Determine which server to use
                let serverName = this.currentServer;
                // If no server selected, try to find a server that provides this tool
                if (!serverName) {
                    serverName = this.serverManager.findToolProvider(toolName);
                    if (!serverName) {
                        return `Error: No server selected and no server found providing tool '${toolName}'. Use "use <server>" first or ensure the tool exists.`;
                    }
                    console.log(`Auto-selected server '${serverName}' for tool '${toolName}'`);
                }
                try {
                    // Execute the tool with progress indicator
                    const result = await this.serverManager.executeTool(serverName, toolName, toolArgs, {
                        showProgress: true
                    });
                    if (result.isError) {
                        return `Error executing tool '${toolName}': ${result.errorMessage}`;
                    }
                    // Format the result output
                    let output = `Tool '${toolName}' execution result from ${serverName}:\n`;
                    output += `Time: ${result.executionTime}ms\n`;
                    output += `Result:\n`;
                    // Format result based on type (assuming result.toolResult is the 'content' array)
                    if (Array.isArray(result.toolResult)) {
                        result.toolResult.forEach(contentItem => {
                            if (contentItem.type === 'text') {
                                output += contentItem.text;
                            }
                            else if (contentItem.type === 'image' || contentItem.type === 'audio') {
                                output += `[${contentItem.type} data (mime: ${contentItem.mimeType})]`;
                            }
                            else {
                                output += JSON.stringify(contentItem); // Fallback for other types
                            }
                        });
                    }
                    else if (typeof result.toolResult === 'object' && result.toolResult !== null) {
                        output += JSON.stringify(result.toolResult, null, 2);
                    }
                    else {
                        output += String(result.toolResult);
                    }
                    return output;
                }
                catch (error) {
                    return `Error: ${error instanceof Error ? error.message : String(error)}`;
                }
            }
        });
        // --- AI/Model Commands ---
        // List providers command
        this.addCommand({
            name: 'providers',
            description: 'List available AI providers and models',
            execute: async () => {
                const output = ['Available AI providers and models:'];
                for (const [providerName, config] of Object.entries(this.providers)) {
                    const isCurrent = providerName === this.currentProvider;
                    const prefix = isCurrent ? '* ' : '  ';
                    output.push(`${prefix}${providerName}: ${config.model || 'No model specified'}`);
                    // List available models for this provider
                    const providerKey = providerName.toLowerCase();
                    const models = this.providerModels[providerKey]?.models || [];
                    if (models.length > 0) {
                        output.push(`    Available models:`);
                        for (const model of models) {
                            const isCurrentModel = config.model === model;
                            output.push(`    ${isCurrentModel ? '* ' : '  '}${model}`);
                        }
                    }
                    else {
                        output.push(`    No model suggestions available for this provider.`);
                    }
                    output.push(''); // Add blank line between providers
                }
                if (Object.keys(this.providers).length === 0) {
                    output.push('No AI providers configured. Please check your ai_config.json file.');
                }
                return output.join('\n');
            }
        });
        // Switch provider command
        this.addCommand({
            name: 'provider',
            description: 'Set the current AI provider',
            execute: async (args) => {
                if (!args[0]) {
                    return 'Error: Provider name required. Use "providers" command to see available providers.';
                }
                const providerName = args[0];
                if (!this.providers[providerName]) {
                    return `Error: Provider '${providerName}' not found in config. Use "providers" command to see available providers.`;
                }
                // Set current provider
                this.currentProvider = providerName;
                // Save updated config to file
                this.saveAiConfig();
                // Switch the AI client
                try {
                    const providerConfig = this.providers[providerName];
                    const modelName = this.conversationManager.switchAiClient(providerConfig, this.providerModels);
                    this.rl.setPrompt(this.getPrompt()); // Update prompt with new model
                    return `Switched to provider: ${providerName} with model: ${modelName}`;
                }
                catch (error) {
                    return `Error switching provider: ${error instanceof Error ? error.message : String(error)}`;
                }
            }
        });
        // Switch model command
        this.addCommand({
            name: 'model',
            description: 'Set the model for the current provider',
            execute: async (args) => {
                if (!this.currentProvider) {
                    return 'Error: No provider selected. Use "provider <name>" command first.';
                }
                if (!args[0]) {
                    return 'Error: Model name required. Use "providers" command to see available models.';
                }
                const modelName = args[0];
                const providerConfig = this.providers[this.currentProvider];
                const providerKey = this.currentProvider.toLowerCase();
                const models = this.providerModels[providerKey]?.models || [];
                // Check if model exists in available models
                if (models.length > 0 && !models.includes(modelName)) {
                    return `Warning: Model '${modelName}' not found in suggested models for provider '${this.currentProvider}'. Continuing anyway.`;
                }
                // Update config
                this.providers[this.currentProvider].model = modelName;
                // Save updated config to file
                this.saveAiConfig();
                // Switch the AI client with the updated model
                try {
                    const updatedModelName = this.conversationManager.switchAiClient(providerConfig, this.providerModels);
                    this.rl.setPrompt(this.getPrompt()); // Update prompt with new model
                    return `Switched model for provider '${this.currentProvider}' to: ${updatedModelName}`;
                }
                catch (error) {
                    return `Error switching model: ${error instanceof Error ? error.message : String(error)}`;
                }
            }
        });
        // --- New/Modified Commands ---
        // Chat command to enter mode
        this.addCommand({
            name: 'chat',
            description: 'Enter interactive chat mode with the AI agent',
            execute: async () => {
                if (this.isChatMode) {
                    return 'Already in chat mode.';
                }
                this.isChatMode = true;
                this.currentServer = null; // Deselect server when entering chat
                this.rl.setPrompt(this.getPrompt());
                return `Entered chat mode with ${this.conversationManager.getAiClientModelName()}. Type 'exit' to leave chat mode.`;
            }
        });
        // History command (only in chat mode)
        this.addCommand({
            name: 'history',
            description: 'Show the current conversation history (chat mode only)',
            execute: async () => {
                if (!this.isChatMode) {
                    return 'The history command is only available in chat mode.';
                }
                const history = this.conversationManager.getHistory();
                if (history.length === 0) {
                    return 'Conversation history is empty.';
                }
                let output = 'Conversation History:\n';
                output += '---------------------\n';
                history.forEach((msg, index) => {
                    const role = msg._getType(); // Get role ('system', 'human', 'ai', 'tool')
                    let contentPreview = '';
                    if (typeof msg.content === 'string') {
                        contentPreview = msg.content.length > 100 ? msg.content.substring(0, 97) + '...' : msg.content;
                    }
                    else {
                        contentPreview = JSON.stringify(msg.content); // Handle complex content
                    }
                    output += `[${index}] ${role.toUpperCase()}: ${contentPreview}\n`;
                });
                output += '---------------------';
                return output;
            }
        });
        // Clear command (only in chat mode)
        this.addCommand({
            name: 'clear',
            description: 'Clear the current conversation history (chat mode only)',
            execute: async () => {
                if (!this.isChatMode) {
                    return 'The clear command is only available in chat mode.';
                }
                this.conversationManager.clearConversation();
                return 'Conversation history cleared.';
            }
        });
        // Exit command (handles both chat mode exit and REPL exit)
        this.addCommand({
            name: 'exit',
            description: 'Exit chat mode or the REPL',
            execute: async () => {
                if (this.isChatMode) {
                    this.isChatMode = false;
                    this.rl.setPrompt(this.getPrompt());
                    return 'Exited chat mode.';
                }
                else {
                    this.stop();
                    return 'Exiting MCP Multi-Client...';
                }
            }
        });
        // Alias for exit
        this.addCommand({
            name: 'quit',
            description: 'Alias for exit',
            execute: async () => {
                // Directly call the exit command's logic
                const exitCommand = this.commands.get('exit');
                if (exitCommand) {
                    return exitCommand.execute([]);
                }
                return ''; // Should not happen
            }
        });
    }
    /**
     * Add a new command to the REPL
     */
    addCommand(command) {
        this.commands.set(command.name, command);
    }
    /**
     * Start the REPL interface
     */
    start() {
        if (this.running)
            return;
        this.running = true;
        console.log('MCP Multi-Client Interactive Console');
        console.log('Type "help" for available commands.');
        console.log('Use "chat" to talk to the AI agent.'); // Updated help text
        console.log('-'.repeat(40));
        this.rl.prompt();
        this.rl.on('line', async (line) => {
            const trimmedLine = line.trim();
            // Skip empty lines
            if (!trimmedLine) {
                this.rl.prompt();
                return;
            }
            // Add to history (basic readline history)
            // this.history.push(trimmedLine);
            // this.historyIndex = this.history.length;
            if (this.isChatMode) {
                // Handle chat input
                if (trimmedLine.toLowerCase() === 'exit' || trimmedLine.toLowerCase() === 'quit') {
                    const result = await this.commands.get('exit').execute([]);
                    console.log(result);
                }
                else if (trimmedLine.toLowerCase() === 'history') {
                    const result = await this.commands.get('history').execute([]);
                    console.log(result);
                }
                else if (trimmedLine.toLowerCase() === 'clear') {
                    const result = await this.commands.get('clear').execute([]);
                    console.log(result);
                }
                else {
                    // Send to conversation manager
                    process.stdout.write('AI is thinking... ');
                    const frames = ['-', '\\', '|', '/'];
                    let i = 0;
                    const thinkingInterval = setInterval(() => {
                        process.stdout.write(`\rAI is thinking... ${frames[i++ % frames.length]}`);
                    }, 100);
                    try {
                        const aiResponse = await this.conversationManager.processUserMessage(trimmedLine);
                        clearInterval(thinkingInterval);
                        process.stdout.write('\r' + ' '.repeat(20) + '\r'); // Clear thinking indicator
                        console.log(`AI: ${aiResponse}`);
                    }
                    catch (error) {
                        clearInterval(thinkingInterval);
                        process.stdout.write('\r' + ' '.repeat(20) + '\r'); // Clear thinking indicator
                        console.error('Chat processing error:', error instanceof Error ? error.message : String(error));
                    }
                }
            }
            else {
                // Handle command input
                const parts = trimmedLine.split(' ');
                const commandName = parts[0].toLowerCase();
                const args = parts.slice(1);
                if (this.commands.has(commandName)) {
                    try {
                        const result = await this.commands.get(commandName).execute(args);
                        if (result)
                            console.log(result);
                    }
                    catch (error) {
                        console.error('Command execution error:', error instanceof Error ? error.message : String(error));
                    }
                }
                else {
                    console.log(`Unknown command: ${commandName}. Type "help" for available commands or "chat" to talk to the AI.`);
                }
            }
            // Update prompt and show it for next input, unless REPL was stopped
            if (this.running) {
                this.rl.setPrompt(this.getPrompt());
                this.rl.prompt();
            }
        });
        // Handle CTRL+C
        this.rl.on('SIGINT', () => {
            if (this.isChatMode) {
                console.log('\nType "exit" or "quit" to leave chat mode.');
                this.rl.prompt();
            }
            else {
                // Default SIGINT behavior (or custom exit prompt)
                console.log('\nUse "exit" or "quit" to exit the REPL. Press Ctrl+C again to force exit.');
                // Maybe exit directly on second Ctrl+C?
                this.rl.prompt(); // Re-prompt in command mode
            }
        });
    }
    /**
     * Stop the REPL interface
     */
    stop() {
        if (!this.running)
            return;
        this.running = false;
        this.rl.close();
        console.log("REPL stopped."); // Add log message
    }
}
//# sourceMappingURL=Repl.js.map