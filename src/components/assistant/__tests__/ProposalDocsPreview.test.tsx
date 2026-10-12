import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ProposalDocsPreview } from '@/components/assistant/ProposalDocsPreview';

describe('ProposalDocsPreview', () => {
  it('renders the Markdown of the proposed docs', () => {
    render(<ProposalDocsPreview markdown={'# Orders\n\n- first item'} />);
    expect(screen.getByText('Docs preview')).toBeInTheDocument();
    expect(screen.getByText('Orders')).toBeInTheDocument();
    expect(screen.getByText('first item')).toBeInTheDocument();
  });

  it('shows plain text for docs without Markdown', () => {
    render(<ProposalDocsPreview markdown='Returns the order list.' />);
    expect(screen.getByText('Returns the order list.')).toBeInTheDocument();
  });
});
