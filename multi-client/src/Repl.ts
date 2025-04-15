import * as readline from 'node:readline';
import { Command } from './types.js';
import { ServerManager } from './ServerManager.js';

/**
 * Interactive REPL for interacting with MCP servers
 */
export class Repl {
  private rl: readline.Interface;
  private commands: Map<string, Command> = new Map();
  private serverManager: ServerManager;
  private currentServer: string | null = null;
  private running = false;
  private history: string[] = [];
  private historyIndex = 0;
  
  constructor(serverManager: ServerManager) {
    this.serverManager = serverManager;
    
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
  
  private getPrompt(): string {
    const serverText = this.currentServer || 'none';
    return `mcp [${serverText}]> `;
  }
  
  private registerCommands() {
    // Help command
    this.addCommand({
      name: 'help',
      description: 'Show available commands',
      execute: async () => {
        let output = 'Available commands:\n';
        for (const [name, cmd] of this.commands.entries()) {
          output += `  ${name.padEnd(12)} - ${cmd.description}\n`;
        }
        return output;
      }
    });
    
    // List servers command
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
    
    // Use server command
    this.addCommand({
      name: 'use',
      description: 'Select a server to use for commands (use <server-name>)',
      execute: async (args) => {
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
    
    // List tools command
    this.addCommand({
      name: 'tools',
      description: 'List available tools on the current server',
      execute: async (args) => {
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
            output += `  ${tool.name} - ${tool.description}\n`;
          });
          
          return output;
        } catch (error) {
          return `Error: ${error instanceof Error ? error.message : String(error)}`;
        }
      }
    });
    
    // Execute tool command
    this.addCommand({
      name: 'call',
      description: 'Execute a tool with arguments (call <tool-name> <json-args>)',
      execute: async (args) => {
        if (!args[0]) {
          return 'Error: Tool name required.';
        }
        
        const toolName = args[0];
        let toolArgs: Record<string, any> = {};
        
        // Parse JSON arguments if provided
        if (args[1]) {
          try {
            toolArgs = JSON.parse(args.slice(1).join(' '));
          } catch (error) {
            return `Error parsing JSON arguments: ${error instanceof Error ? error.message : String(error)}`;
          }
        }
        
        // Determine which server to use
        let serverName = this.currentServer;
        
        // If no server selected, try to find a server that provides this tool
        if (!serverName) {
          serverName = this.serverManager.findToolProvider(toolName);
          if (!serverName) {
            return `Error: No server selected and no server found providing tool '${toolName}'.`;
          }
          console.log(`Using server '${serverName}' for tool '${toolName}'`);
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
          let output = `Tool '${toolName}' execution result:\n`;
          output += `Time: ${result.executionTime}ms\n`;
          output += `Result:\n`;
          
          // Format result based on type
          if (typeof result.toolResult === 'object' && result.toolResult !== null) {
            output += JSON.stringify(result.toolResult, null, 2);
          } else {
            output += String(result.toolResult);
          }
          
          return output;
        } catch (error) {
          return `Error: ${error instanceof Error ? error.message : String(error)}`;
        }
      }
    });
    
    // Exit command
    this.addCommand({
      name: 'exit',
      description: 'Exit the REPL',
      execute: async () => {
        this.stop();
        return 'Exiting...';
      }
    });
    
    // Alias for exit
    this.addCommand({
      name: 'quit',
      description: 'Alias for exit',
      execute: async () => {
        return this.commands.get('exit')!.execute([]);
      }
    });
  }
  
  /**
   * Add a new command to the REPL
   */
  addCommand(command: Command) {
    this.commands.set(command.name, command);
  }
  
  /**
   * Start the REPL interface
   */
  start() {
    if (this.running) return;
    this.running = true;
    
    console.log('MCP Multi-Client Interactive Console');
    console.log('Type "help" for available commands.');
    console.log('-'.repeat(40));
    
    this.rl.prompt();
    
    this.rl.on('line', async (line) => {
      const trimmedLine = line.trim();
      
      // Skip empty lines
      if (!trimmedLine) {
        this.rl.prompt();
        return;
      }
      
      // Add to history
      this.history.push(trimmedLine);
      this.historyIndex = this.history.length;
      
      // Parse command and arguments
      const parts = trimmedLine.split(' ');
      const commandName = parts[0].toLowerCase();
      const args = parts.slice(1);
      
      // Execute command if registered
      if (this.commands.has(commandName)) {
        try {
          const result = await this.commands.get(commandName)!.execute(args);
          if (result) console.log(result);
        } catch (error) {
          console.error('Command execution error:', error instanceof Error ? error.message : String(error));
        }
      } else {
        console.log(`Unknown command: ${commandName}. Type "help" for available commands.`);
      }
      
      // Update prompt and show it for next input, unless REPL was stopped
      if (this.running) {
        this.rl.setPrompt(this.getPrompt());
        this.rl.prompt();
      }
    });
    
    // Handle CTRL+C
    this.rl.on('SIGINT', () => {
      console.log('\nUse "exit" or "quit" to exit the REPL');
      this.rl.prompt();
    });
  }
  
  /**
   * Stop the REPL interface
   */
  stop() {
    this.running = false;
    this.rl.close();
  }
}
