import type { Command, ProviderModelsStructure } from './types.js';
import type { ServerManager } from './ServerManager.js';
import type { ConversationManager } from './conversation/ConversationManager.js';
export declare class Repl {
    private rl;
    private commands;
    private serverManager;
    private conversationManager;
    private currentServer;
    private isChatMode;
    private running;
    private providers;
    private providerModels;
    private currentProvider;
    private aiConfigPath;
    constructor(serverManager: ServerManager, conversationManager: ConversationManager, providerModels: ProviderModelsStructure);
    /**
     * Load the AI provider configuration from the config file
     */
    private loadAiConfig;
    /**
     * Save the AI provider configuration to the config file
     */
    private saveAiConfig;
    private getPrompt;
    private registerCommands;
    /**
     * Add a new command to the REPL
     */
    addCommand(command: Command): void;
    /**
     * Start the REPL interface
     */
    start(): void;
    /**
     * Stop the REPL interface
     */
    stop(): void;
}
