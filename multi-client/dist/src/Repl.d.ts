import type { Command } from './types.js';
import type { ServerManager } from './ServerManager.js';
import type { ConversationManager } from './conversation/ConversationManager.js';
/**
 * Interactive REPL for interacting with MCP servers and AI agent
 */
export declare class Repl {
    private rl;
    private commands;
    private serverManager;
    private conversationManager;
    private currentServer;
    private isChatMode;
    private running;
    constructor(serverManager: ServerManager, conversationManager: ConversationManager);
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
