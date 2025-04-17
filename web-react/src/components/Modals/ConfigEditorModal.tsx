import { useState, useEffect, useCallback } from 'react'; // Removed unused React import
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faTimes } from '@fortawesome/free-solid-svg-icons';
import { useStore } from '@/store/store';
import { shallow } from 'zustand/shallow';
import toast from 'react-hot-toast';
import Spinner from '@/components/common/Spinner';
import { fetchConfigFileApi, saveConfigFileApi } from '@/services/api';

const ConfigEditorModal: React.FC = () => {
    const {
        isConfigEditorOpen,
        closeConfigEditor,
        currentEditingConfigFile,
        fetchProviders, // To refresh AI config if needed
        fetchServerConfig, // To refresh server config if needed
    } = useStore(
        (state) => ({
            isConfigEditorOpen: state.isConfigEditorOpen,
            closeConfigEditor: state.closeConfigEditor,
            currentEditingConfigFile: state.currentEditingConfigFile,
            fetchProviders: state.fetchProviders,
            fetchServerConfig: state.fetchServerConfig,
        }),
        shallow
    );

    const [content, setContent] = useState<string>('');
    const [isLoading, setIsLoading] = useState<boolean>(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (isConfigEditorOpen && currentEditingConfigFile) {
            setIsLoading(true);
            setError(null);
            setContent(''); // Clear previous content
            fetchConfigFileApi(currentEditingConfigFile)
                .then(data => {
                    let formattedContent = data.content;
                    // Pretty print JSON
                    if (currentEditingConfigFile.endsWith('.json')) {
                        try {
                            formattedContent = JSON.stringify(JSON.parse(formattedContent), null, 2);
                        } catch { /* Ignore parse error, show raw */ }
                    }
                    setContent(formattedContent);
                })
                .catch(err => {
                    setError(`Failed to load ${currentEditingConfigFile}: ${err.message}`);
                    toast.error(`Failed to load ${currentEditingConfigFile}`);
                })
                .finally(() => setIsLoading(false));
        }
    }, [isConfigEditorOpen, currentEditingConfigFile]);

    const handleSave = useCallback(async () => {
        if (!currentEditingConfigFile) return;

        // Basic client-side validation
        if (currentEditingConfigFile.endsWith('.json')) {
            try {
                JSON.parse(content);
            } catch (e: any) {
                toast.error(`Invalid JSON: ${e.message}`);
                return;
            }
        }
        // TOML validation is harder client-side, rely on backend/API validation

        setIsLoading(true);
        setError(null);
        try {
            const result = await saveConfigFileApi(currentEditingConfigFile, content);
            // Success toast handled by API client
            if (result.success) {
                closeConfigEditor();
                // Trigger data refresh if necessary
                if (currentEditingConfigFile === 'ai_config.json') {
                    fetchProviders();
                } else if (currentEditingConfigFile === 'servers.json') {
                    // Fetching server config might not reflect immediate changes
                    // as backend restart is needed, but good to refresh UI state.
                    fetchServerConfig();
                }
            }
        } catch (err: any) {
            setError(`Failed to save: ${err.message}`);
            // Error toast handled by API client
        } finally {
            setIsLoading(false);
        }
    }, [currentEditingConfigFile, content, closeConfigEditor, fetchProviders, fetchServerConfig]);

    if (!isConfigEditorOpen) return null;

    return (
        <div className="modal open">
            <div className="modal-content max-w-4xl w-full">
                {isLoading && <Spinner message={content ? 'Saving...' : 'Loading...'} />}
                <div className="flex justify-between items-center mb-5">
                    <h2 className="text-xl font-bold text-gray-800 dark:text-white">
                        Edit: <span className="font-mono text-primary">{currentEditingConfigFile || 'Configuration File'}</span>
                    </h2>
                    <button onClick={closeConfigEditor} className="modal-close" title="Close">
                        <FontAwesomeIcon icon={faTimes} />
                    </button>
                </div>

                <div className="space-y-4">
                    <div className="code-editor-wrapper border border-gray-300 dark:border-gray-600 rounded-lg overflow-hidden bg-gray-50 dark:bg-gray-900 h-96">
                        <textarea
                            id="config-editor"
                            value={content}
                            onChange={(e) => setContent(e.target.value)}
                            disabled={isLoading}
                            className="w-full h-full p-4 font-mono text-sm bg-transparent text-gray-800 dark:text-gray-100 focus:outline-none resize-none"
                            placeholder={isLoading ? 'Loading...' : 'File content...'}
                            spellCheck="false"
                        />
                    </div>
                    {error && (
                        <div className="text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/30 p-3 rounded-lg">
                            <b>Error:</b> {error}
                        </div>
                    )}
                    <div className="text-xs text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-900/30 p-3 rounded-lg">
                        <b>Warning:</b> Invalid syntax may cause the application to malfunction. A restart is often required for changes to take effect.
                    </div>

                    <div className="flex justify-end gap-3 pt-4 border-t border-gray-200 dark:border-gray-700">
                        <button
                            onClick={closeConfigEditor}
                            disabled={isLoading}
                            className="btn btn-secondary py-2 px-4"
                            type="button"
                        >
                            Cancel
                        </button>
                        <button
                            onClick={handleSave}
                            disabled={isLoading}
                            className="btn btn-primary py-2 px-4"
                            type="button"
                        >
                            {isLoading ? 'Saving...' : 'Save Changes'}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
};

export default ConfigEditorModal;
