import type { IAiClient } from '../../ai/IAiClient.js';
import { PromptFactory } from '../prompts/PromptFactory.js';
import { SystemMessage, HumanMessage } from '../Message.js';
import type { ConversationMessage } from '../Message.js';

export interface VerificationResult {
    passes: boolean;
    feedback: string;
}

export class VerificationService {
    private aiClient: IAiClient;
    private promptFactory: typeof PromptFactory;

    constructor(aiClient: IAiClient) {
        this.aiClient = aiClient;
        this.promptFactory = PromptFactory; // Use static prompts
    }

    /**
     * Generates verification criteria for a user request using the AI client.
     * @param userInput The original user input/request.
     * @returns The generated verification criteria string.
     */
    public async generateVerificationCriteria(userInput: string): Promise<string> {
        console.log('[VerificationService] Generating verification criteria...');
        try {
            // Create the criteria prompt using the factory
            const promptText = this.promptFactory.fillVerificationCriteriaPrompt(userInput);

            // Create a temporary message list for this specific AI call
            const criteriaMessages: ConversationMessage[] = [
                new SystemMessage("You are a helpful assistant that generates verification criteria."), // System context for this task
                new HumanMessage(promptText)
            ];

            // Call the AI client
            const criteriaResponse = await this.aiClient.generateResponse(criteriaMessages);
            console.log('[VerificationService] Generated criteria:', criteriaResponse);
            return criteriaResponse;
        } catch (error) {
            console.error('[VerificationService] Error generating verification criteria:', error);
            // Provide a default fallback criteria on error
            return '- Respond to the user\'s request accurately.\n- Provide relevant information.';
        }
    }

    /**
     * Verifies an AI response against the criteria using the AI client.
     * @param originalRequest The original user request.
     * @param criteria The verification criteria.
     * @param relevantSequence The formatted conversation sequence to verify.
     * @returns Object with verification result (`passes`) and `feedback`.
     */
    public async verifyResponse(
        originalRequest: string,
        criteria: string,
        relevantSequence: string
    ): Promise<VerificationResult> {
        console.log('[VerificationService] Verifying response against criteria...');

        try {
            // Create the verification prompt using the factory
            const promptText = this.promptFactory.fillVerificationPrompt(originalRequest, criteria, relevantSequence);

            // Create a temporary message list for this specific AI call
            const verificationMessages: ConversationMessage[] = [
                new SystemMessage("You are a strict evaluator that verifies responses against criteria and returns JSON."), // System context for this task
                new HumanMessage(promptText)
            ];

            // Call the AI client
            const verificationResponse = await this.aiClient.generateResponse(verificationMessages);

            // Attempt to parse the JSON response from the AI
            try {
                const result = JSON.parse(verificationResponse);
                if (typeof result === 'object' && result !== null && 'passes' in result) {
                    console.log('[VerificationService] Verification result:', result.passes ? 'PASSED' : 'FAILED');
                    if (!result.passes) {
                        console.log('[VerificationService] Feedback:', result.feedback);
                    }
                    return {
                        passes: Boolean(result.passes),
                        feedback: result.feedback || ''
                    };
                } else {
                    console.warn('[VerificationService] Invalid verification response format:', verificationResponse);
                    return { passes: true, feedback: 'Invalid format from verifier' }; // Default to passing but note format issue
                }
            } catch (parseError) {
                console.error('[VerificationService] Error parsing verification response JSON:', parseError);
                console.log('[VerificationService] Raw verification response:', verificationResponse);
                // Fail verification if the response format is invalid
                return { passes: false, feedback: 'Failed to parse verifier response JSON. Assuming failure.' };
            }
        } catch (aiError) {
            console.error('[VerificationService] Error during AI verification call:', aiError);
            // Fail verification if the verifier AI call fails
            return { passes: false, feedback: `Verifier AI call failed: ${aiError instanceof Error ? aiError.message : String(aiError)}` };
        }
    }

     /**
      * Generates a revised response based on verification failure feedback.
      * @param currentHistory The conversation history leading up to the failed response.
      * @param failedResponseContent The content of the response that failed verification.
      * @param feedback The feedback from the verification process.
      * @returns The revised response content from the AI.
      * @throws {Error} If the AI call for correction fails.
      */
     public async generateCorrectedResponse(
         currentHistory: ConversationMessage[],
         failedResponseContent: string,
         feedback: string
     ): Promise<string> {
         console.log('[VerificationService] Generating corrected response...');

         // Generate correction prompt using the factory
         const correctionPromptText = this.promptFactory.fillVerificationFailurePrompt(feedback);

         // Create messages for the correction call:
         // History up to the point *before* the failed final response + the failed response + the correction request
         const correctionMessages = [
             ...currentHistory, // History includes the failed response added in the loop
             new HumanMessage(correctionPromptText) // Add the correction request
         ];

         try {
             const correctedResponse = await this.aiClient.generateResponse(correctionMessages);
             console.log('[VerificationService] Generated corrected response after verification failure.');
             return correctedResponse;
         } catch (error) {
             console.error('[VerificationService] Error generating corrected response:', error);
             throw new Error(`Failed to generate corrected response: ${error instanceof Error ? error.message : String(error)}`);
         }
     }
}
