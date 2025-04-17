import React from 'react';
import { useStore, StoreType } from '@/store/store'; // Import StoreType
import { shallow } from 'zustand/shallow'; // Import shallow

interface MainLayoutProps {
  children: React.ReactNode;
}

const MainLayout: React.FC<MainLayoutProps> = ({ children }) => {
    // Select multiple properties - use shallow
    const { isPanelCollapsed, isPanelOpen } = useStore(
        (state: StoreType) => ({ // Type state
            isPanelCollapsed: state.isPanelCollapsed,
            isPanelOpen: state.isPanelOpen,
        }),
        shallow
    );

    // Determine classes based on panel state and screen size
    // We apply classes based on state, and media queries in CSS handle the visual change.
    // Or use a library like react-responsive if complex logic is needed.
    const mainClasses = `flex-grow flex container mx-auto px-4 py-6 gap-6 relative ${
        isPanelCollapsed ? 'panel-collapsed' : '' // Class for large screens when collapsed
    } ${
        isPanelOpen ? 'panel-open' : '' // Class for small screens when open
    }`;


  return (
    <main className={mainClasses}>
      {children}
    </main>
  );
};

export default MainLayout;
