import type { IAiClient } from '../ai/IAiClient.js';
import type { ServerManager } from '../ServerManager.js';
import { ConversationState } from './ConversationState.js';
import { HumanMessage, AIMessage, SystemMessage } from './Message.js';
import type { ConversationMessage } from './Message.js';
import type { Tool } from '@modelcontextprotocol/sdk/types.js'; // MCP Tool type

export class ConversationManager {
  private state: ConversationState;
  private aiClient: IAiClient;
  private serverManager: ServerManager;

  constructor(aiClient: IAiClient, serverManager: ServerManager) {
    this.aiClient = aiClient;
    this.serverManager = serverManager;
    this.state = new ConversationState(); // Initialize with no system prompt initially
    // System prompt will be generated dynamically based on tools
  }

  /**
   * Gets the model name identifier from the underlying AI client.
   */
  public getAiClientModelName(): string {
    return this.aiClient.getModelName();
  }

  /**
   * Generates the system prompt including tool definitions.
   * TODO: Refine prompt text in Phase 2 to match finish_implementation.md exactly.
   */
  private async generateToolSystemPrompt(): Promise<string> {
    const allTools: Tool[] = await this.serverManager.getAllTools(); // Assumes ServerManager has this method

    // Basic prompt structure - will be enhanced in Phase 2
    let prompt = `You are a helpful assistant. Use tools if necessary.

Available tools:
`;

    if (allTools.length === 0) {
      prompt += "No tools available.\n";
    } else {
      const toolsInfo = allTools
        .map((t) => {
          const schemaString = t.input_schema
            ? JSON.stringify(t.input_schema)
            : '{}';
          // Ensure description is a string
          const description = typeof t.description === 'string' ? t.description : (t.description?.toString() ?? 'No description');
          return `- Name: ${t.name}\n  Description: ${description}\n  Schema: ${schemaString}`;
        })
        .join('\n\n');
      prompt += toolsInfo;
    }

    // Add tool usage format instructions (basic version for now)
    prompt += `\n\nWhen you need to use a tool, respond *only* with the following format inside delimiters:
<<<TOOL_CALL>>>
{
  "name": "tool_name",
  "arguments": { /* JSON arguments matching the tool's schema */ }
}
<<<END_TOOL_CALL>>>`;

    return prompt;
  }

  /**
   * Processes a user's message, interacts with the AI, and potentially handles tool calls (in later phases).
   * @param userInput - The text input from the user.
   * @returns The AI's final response for this turn.
   */
  async processUserMessage(userInput: string): Promise<string> {
    console.log(`Processing user message: "${userInput}"`);

    // 1. Add user message to state
    this.state.addMessage(new HumanMessage(userInput));

    // 2. Generate the dynamic system prompt based on current tools
    const systemPrompt = await this.generateToolSystemPrompt();
    this.state.setSystemPrompt(systemPrompt); // Update the system prompt in the state

    // 3. Get the full message history for the AI
    const messagesForAi = this.state.getMessages();
    // Avoid logging potentially large system prompt every time in production
    // console.log(`Messages sent to AI (${this.aiClient.getModelName()}):`, messagesForAi.map(m => ({ role: m._getType(), content: m.content })));


    // 4. Call the AI
    try {
      const aiResponseContent = await this.aiClient.generateResponse(messagesForAi);
      console.log(`AI Response (${this.aiClient.getModelName()}): "${aiResponseContent}"`);

      // --- Placeholder for Phase 2: Tool Parsing & Execution ---
      // TODO: Parse aiResponseContent for <<<TOOL_CALL>>> blocks
      // TODO: If tool calls found:
      //   - Execute tools using serverManager.executeTool
      //   - Format results
      //   - Add results to state (as ToolMessage or similar)
      //   - Call AI again with history + tool results + "Tool Results Processing Prompt"
      //   - Update aiResponseContent with the result of the second AI call
      // TODO: Handle invalid tool call format (call AI again with feedback prompt)
      // --- End Placeholder ---

      // 5. Add final AI response to state
      this.state.addMessage(new AIMessage(aiResponseContent));

      // --- Placeholder for Phase 3: Verification ---
      // TODO: If this is a final response (no tools called):
      //   - Generate criteria (if first turn)
      //   - Verify response against criteria
      //   - If verification fails, add feedback and call AI again
      // --- End Placeholder ---


      // 6. Return the final AI response for this turn
      return aiResponseContent;

    } catch (error) {
      console.error("Error during AI interaction:", error);
      // Add an error message to the conversation state? Or just return error?
      const errorMessage = `Sorry, I encountered an error: ${error instanceof Error ? error.message : String(error)}`;
      // Optionally add this error as an AI message to keep track
      // this.state.addMessage(new AIMessage(errorMessage));
      return errorMessage; // Return error message to the user
    }
  }

  /**
   * Clears the conversation history.
   */
  clearConversation(): void {
    this.state.clearHistory();
    console.log("Conversation history cleared.");
  }

  /**
   * Gets the current conversation history.
   */
  getHistory(): ConversationMessage[] {
    // Return messages including the system prompt for context
    return this.state.getMessages();
  }
}
