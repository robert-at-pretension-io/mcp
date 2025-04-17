import React from 'react';
import { useStore } from '@/store/store';

interface MainLayoutProps {
  children: React.ReactNode;
}

const MainLayout: React.FC<MainLayoutProps> = ({ children }) => {
    const isPanelCollapsed = useStore((state) => state.isPanelCollapsed);
    const isPanelOpen = useStore((state) => state.isPanelOpen); // For mobile

    // Determine classes based on panel state and screen size
    // Note: Tailwind doesn't directly support conditional classes based on JS logic for breakpoints easily.
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