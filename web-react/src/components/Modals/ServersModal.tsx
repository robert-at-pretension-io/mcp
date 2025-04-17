import { useState, useEffect, useCallback } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faTimes, faPlus, faTrash } from '@fortawesome/free-solid-svg-icons';
import { useStore, ServerConfig, StoreType } from '@/store/store';
import { shallow } from 'zustand/shallow'; // Keep shallow
import toast from 'react-hot-toast';
import Spinner from '@/components/common/Spinner';
import { escapeHtml } from '@/utils/helpers';

const ServersModal: React.FC = () => {
    const {
        isServersModalOpen,
        closeServersModal,
        serverConfig,
        selectedServerName,
        setSelectedServerName,
        setServerConfig, // Action to update the config in the store
        saveServerConfig, // Action to save to backend
        fetchServerConfig, // Action to fetch initial config
    } = useStore(
        (state: StoreType) => ({
            isServersModalOpen: state.isServersModalOpen,
            closeServersModal: state.closeServersModal,
            serverConfig: state.serverConfig,
            selectedServerName: state.selectedServerName,
            setSelectedServerName: state.setSelectedServerName,
            setServerConfig: state.setServerConfig,
            saveServerConfig: state.saveServerConfig,
            fetchServerConfig: state.fetchServerConfig,
        }),
        shallow // Use shallow since we select an object
    );

    const [isLoading, setIsLoading] = useState<boolean>(false);
    const [isEditing, setIsEditing] = useState<boolean>(false); // Track if form is visible

    // Local state for the form fields
    const [formServerName, setFormServerName] = useState<string>('');
    const [formCommand, setFormCommand] = useState<string>('');
    const [formArgs, setFormArgs] = useState<string[]>([]);
    const [formEnv, setFormEnv] = useState<Record<string, string>>({});

    // Fetch config when modal opens
    useEffect(() => {
        if (isServersModalOpen) {
            fetchServerConfig(); // Fetches and updates store's serverConfig
            setIsEditing(false); // Hide form initially
            setSelectedServerName(null); // Deselect server on open
        }
    }, [isServersModalOpen, fetchServerConfig, setSelectedServerName]);

    // Populate form when a server is selected
    useEffect(() => {
        if (selectedServerName && serverConfig.mcpServers?.[selectedServerName]) {
            const config = serverConfig.mcpServers[selectedServerName];
            setFormServerName(selectedServerName);
            setFormCommand(config.command || '');
            setFormArgs(config.args || []);
            setFormEnv(config.env || {});
            setIsEditing(true);
        } else {
            setIsEditing(false); // Hide form if no server selected or config missing
        }
    }, [selectedServerName, serverConfig]);

    const handleSelectServer = (name: string) => {
        setSelectedServerName(name);
    };

    const handleAddNewServer = () => {
        let newName = 'new-server';
        let i = 1;
        while (serverConfig.mcpServers?.[newName]) {
            newName = `new-server-${i++}`;
        }
        // Create a temporary new config in the store
        const newConfig = { ...serverConfig };
        if (!newConfig.mcpServers) newConfig.mcpServers = {};
        newConfig.mcpServers[newName] = { command: '', args: [], env: {} };
        setServerConfig(newConfig); // Update store immediately
        setSelectedServerName(newName); // Select the new server
    };

    const handleDeleteServer = (nameToDelete: string) => {
        if (window.confirm(`Delete server "${nameToDelete}" configuration? Restart required after saving.`)) {
            const newConfig = { ...serverConfig };
            if (newConfig.mcpServers) {
                delete newConfig.mcpServers[nameToDelete];
                setServerConfig(newConfig); // Update store
                if (selectedServerName === nameToDelete) {
                    setSelectedServerName(null); // Deselect if deleted
                }
            }
        }
    };

    // --- Form Field Handlers ---
    const handleArgChange = (index: number, value: string) => {
        const newArgs = [...formArgs];
        newArgs[index] = value;
        setFormArgs(newArgs);
    };
    const handleAddArg = () => setFormArgs([...formArgs, '']);
    const handleRemoveArg = (index: number) => setFormArgs(formArgs.filter((_, i) => i !== index));

    const handleEnvKeyChange = (index: number, newKey: string) => {
        const entries = Object.entries(formEnv);
        if (index < entries.length) {
            const [oldKey, value] = entries[index];
            const newEntries = entries.filter(([k]) => k !== oldKey);
            setFormEnv(Object.fromEntries([...newEntries, [newKey, value]]));
        }
    };
    const handleEnvValueChange = (key: string, newValue: string) => {
        setFormEnv({ ...formEnv, [key]: newValue });
    };
    const handleAddEnv = () => setFormEnv({ ...formEnv, '': '' }); // Add empty entry
    const handleRemoveEnv = (keyToRemove: string) => {
        const newEnv = { ...formEnv };
        delete newEnv[keyToRemove];
        setFormEnv(newEnv);
    };
    // --- End Form Field Handlers ---

    const updateConfigFromForm = (): boolean => {
        const currentName = formServerName.trim();
        const command = formCommand.trim();

        if (!currentName || !command) {
            toast.error('Server name and command are required.');
            return false;
        }

        const args = formArgs.map(arg => arg.trim()).filter(Boolean);
        const env: Record<string, string> = {};
        Object.entries(formEnv).forEach(([key, value]) => {
            const trimmedKey = key.trim();
            if (trimmedKey) { // Only add if key is not empty
                env[trimmedKey] = value.trim();
            }
        });

        const newConfig = { ...serverConfig };
        if (!newConfig.mcpServers) newConfig.mcpServers = {};

        // Handle potential rename
        if (selectedServerName && selectedServerName !== currentName) {
            delete newConfig.mcpServers[selectedServerName];
        }

        newConfig.mcpServers[currentName] = { command, args, env };
        setServerConfig(newConfig); // Update store state
        setSelectedServerName(currentName); // Ensure selection matches current name
        return true;
    };


    const handleSaveChanges = useCallback(async () => {
        if (isEditing) { // Only update if form was visible
            if (!updateConfigFromForm()) {
                return; // Validation failed
            }
        }

        setIsLoading(true);
        try {
            await saveServerConfig(serverConfig); // Save the config from the store
            // Success toast handled by store action/API client
            closeServersModal();
        } catch (error) {
            // Error toast handled by store action/API client
        } finally {
            setIsLoading(false);
        }
    }, [serverConfig, saveServerConfig, closeServersModal, isEditing, updateConfigFromForm]);


    if (!isServersModalOpen) return null;

    const serverNames = Object.keys(serverConfig.mcpServers || {}).sort();

    return (
        <div className="modal open">
            <div className="modal-content max-w-4xl w-full">
                {isLoading && <Spinner message="Saving..." />}
                <div className="flex justify-between items-center mb-5">
                    <h2 className="text-xl font-bold text-gray-800 dark:text-white">Manage MCP Servers</h2>
                    <button onClick={closeServersModal} className="modal-close" title="Close">
                        <FontAwesomeIcon icon={faTimes} />
                    </button>
                </div>

                <div className="space-y-6">
                    <div className="server-editor border border-gray-200 dark:border-gray-700 rounded-xl overflow-hidden grid grid-cols-1 lg:grid-cols-3 min-h-[400px]">
                        {/* Server List */}
                        <div className="server-list bg-gray-50 dark:bg-gray-700 border-b lg:border-b-0 lg:border-r border-gray-200 dark:border-gray-600 flex flex-col">
                            <div className="server-list-header flex items-center justify-between p-3 border-b border-gray-200 dark:border-gray-600 flex-shrink-0">
                                <h4 className="font-medium text-gray-800 dark:text-white">Servers</h4>
                                <button onClick={handleAddNewServer} className="btn-icon p-1" title="Add new server">
                                    <FontAwesomeIcon icon={faPlus} />
                                </button>
                            </div>
                            <ul className="flex-grow overflow-y-auto p-2 space-y-1">
                                {serverNames.length === 0 ? (
                                    <li className="p-2 text-sm text-gray-500 text-center">No servers</li>
                                ) : (
                                    serverNames.map(name => (
                                        <li
                                            key={name}
                                            onClick={() => handleSelectServer(name)}
                                            className={`flex justify-between items-center p-2 rounded-md cursor-pointer text-sm hover:bg-gray-200 dark:hover:bg-gray-600 ${selectedServerName === name ? 'bg-primary/10 dark:bg-primary/20 font-semibold' : ''}`}
                                        >
                                            <span>{name}</span>
                                            <button
                                                onClick={(e) => { e.stopPropagation(); handleDeleteServer(name); }}
                                                className="btn-icon text-danger hover:bg-danger/10 p-1 text-xs"
                                                title="Delete server"
                                            >
                                                <FontAwesomeIcon icon={faTrash} />
                                            </button>
                                        </li>
                                    ))
                                )}
                            </ul>
                        </div>

                        {/* Server Details Form */}
                        <div className="server-details col-span-2 p-5 bg-white dark:bg-gray-800 overflow-y-auto">
                            {!isEditing ? (
                                <div className="flex items-center justify-center h-full text-gray-500 dark:text-gray-400">
                                    <p>Select a server or add a new one.</p>
                                </div>
                            ) : (
                                <div className="space-y-5">
                                    {/* Server Name */}
                                    <div className="space-y-1">
                                        <label htmlFor="server-name" className="block text-sm font-medium text-gray-700 dark:text-gray-300">Server Name</label>
                                        <input type="text" id="server-name" value={formServerName} onChange={(e) => setFormServerName(e.target.value)} placeholder="e.g., bash, search" className="form-input" />
                                    </div>
                                    {/* Command */}
                                    <div className="space-y-1">
                                        <label htmlFor="server-command" className="block text-sm font-medium text-gray-700 dark:text-gray-300">Command</label>
                                        <input type="text" id="server-command" value={formCommand} onChange={(e) => setFormCommand(e.target.value)} placeholder="e.g., npx, python" className="form-input" />
                                    </div>
                                    {/* Arguments */}
                                    <div className="space-y-2">
                                        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">Arguments</label>
                                        <div className="space-y-2">
                                            {formArgs.map((arg, index) => (
                                                <div key={index} className="flex items-center gap-2">
                                                    <input type="text" value={arg} onChange={(e) => handleArgChange(index, e.target.value)} placeholder="Argument value" className="form-input flex-grow" />
                                                    <button onClick={() => handleRemoveArg(index)} className="btn-icon text-danger hover:bg-danger/10 p-1" title="Remove argument"><FontAwesomeIcon icon={faTimes} /></button>
                                                </div>
                                            ))}
                                        </div>
                                        <button onClick={handleAddArg} className="btn btn-secondary btn-sm mt-1" type="button"><FontAwesomeIcon icon={faPlus} className="mr-1" /> Add Argument</button>
                                    </div>
                                    {/* Environment Variables */}
                                    <div className="space-y-2">
                                        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">Environment Variables</label>
                                        <div className="space-y-2">
                                            {Object.entries(formEnv).map(([key, value], index) => (
                                                <div key={key + index} className="flex items-center gap-2">
                                                    <input type="text" value={key} onChange={(e) => handleEnvKeyChange(index, e.target.value)} placeholder="Key" className="form-input w-1/3" />
                                                    <input type="text" value={value} onChange={(e) => handleEnvValueChange(key, e.target.value)} placeholder="Value" className="form-input flex-grow" />
                                                    <button onClick={() => handleRemoveEnv(key)} className="btn-icon text-danger hover:bg-danger/10 p-1" title="Remove variable"><FontAwesomeIcon icon={faTimes} /></button>
                                                </div>
                                            ))}
                                        </div>
                                        <button onClick={handleAddEnv} className="btn btn-secondary btn-sm mt-1" type="button"><FontAwesomeIcon icon={faPlus} className="mr-1" /> Add Variable</button>
                                    </div>
                                </div>
                            )}
                        </div>
                    </div>
                    <div className="text-xs text-gray-500 dark:text-gray-400 italic mt-2">
                        <b>Note:</b> Changes require restarting the application to take effect.
                    </div>
                </div>

                {/* Action Buttons */}
                <div className="flex justify-end gap-3 pt-4 border-t border-gray-200 dark:border-gray-700">
                    <button onClick={closeServersModal} disabled={isLoading} className="btn btn-secondary py-2 px-4" type="button">Cancel</button>
                    <button onClick={handleSaveChanges} disabled={isLoading} className="btn btn-primary py-2 px-4" type="button">
                        {isLoading ? 'Saving...' : 'Save Changes'}
                    </button>
                </div>
            </div>
        </div>
    );
};

export default ServersModal;
