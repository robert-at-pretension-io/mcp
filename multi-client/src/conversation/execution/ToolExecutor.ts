import type { ServerManager } from '../../ServerManager.js';
import type { ToolExecutionResult } from '../../types.js';

// Type for the tool calls expected by this executor
export type ToolCallRequest = {
    id: string;
    name: string;
    args: Record<string, any>;
};

export class ToolExecutor {
    private serverManager: ServerManager;

    constructor(serverManager: ServerManager) {
        this.serverManager = serverManager;
    }

    /**
     * Executes a set of tool calls in parallel.
     * @param toolCalls Array of tool calls, each including an `id`, `name`, and `args`.
     * @returns Promise that resolves to a map of tool call IDs to their string results.
     */
    public async executeToolCalls(
        toolCalls: ToolCallRequest[]
    ): Promise<Map<string, string>> {
        const results = new Map<string, string>();
        const executions = toolCalls.map(async (toolCall) => {
            const toolCallId = toolCall.id;
            const toolName = toolCall.name;
            const toolArgs = toolCall.args;

            try {
                const serverName = this.serverManager.findToolProvider(toolName);
                if (!serverName) {
                    const errorMessage = `No server found providing tool '${toolName}'.`;
                    console.warn(`[ToolExecutor] ${errorMessage}`);
                    results.set(toolCallId, errorMessage);
                    return; // Skip execution for this tool call
                }

                console.log(`[ToolExecutor] Executing tool '${toolName}' (ID: ${toolCallId}) on server '${serverName}'...`);

                // Execute the tool using name and args from toolCall
                // Pass showProgress: true for REPL-like feedback if desired, or make it configurable
                const executionResult: ToolExecutionResult = await this.serverManager.executeTool(
                    serverName,
                    toolName,
                    toolArgs,
                    { showProgress: false } // Assuming progress shown elsewhere or not needed here
                );

                // Format the result content
                let resultContent: string;
                if (executionResult.isError) {
                    resultContent = `Error: ${executionResult.errorMessage || 'Unknown error.'}`;
                } else if (Array.isArray(executionResult.toolResult)) {
                    // Handle content array from the tool result
                    resultContent = executionResult.toolResult
                        .map(item => {
                            // Ensure item is an object before accessing properties
                            if (typeof item === 'object' && item !== null) {
                                if (item.type === 'text') {
                                    return item.text;
                                } else if (item.type === 'image') {
                                    return `[Image: ${item.mimeType}]`; // Placeholder for image
                                }
                            }
                            // Fallback for non-object items or unknown types
                            return JSON.stringify(item);
                        })
                        .join('\n');
                } else if (typeof executionResult.toolResult === 'object' && executionResult.toolResult !== null) {
                    resultContent = JSON.stringify(executionResult.toolResult, null, 2);
                } else {
                    resultContent = String(executionResult.toolResult ?? 'No content returned.'); // Use ?? for null/undefined
                }

                console.log(`[ToolExecutor] Result for tool ${toolCallId} (${toolName}):`, JSON.stringify(resultContent).substring(0, 100) + '...');
                results.set(toolCallId, resultContent);

            } catch (error) {
                const errorMessage = `[ToolExecutor] Failed to execute tool '${toolCall.name}' (ID: ${toolCallId}): ${error instanceof Error ? error.message : String(error)}`;
                console.error(errorMessage);
                results.set(toolCallId, errorMessage); // Store error message as result
            }
        });

        // Wait for all tool executions to complete
        await Promise.all(executions);
        return results;
    }
}
