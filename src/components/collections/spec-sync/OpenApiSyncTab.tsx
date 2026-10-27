import { SpecSyncTab } from './SpecSyncTab';

interface OpenApiSyncTabProps {
  collection: string;
}

/** The dedicated OpenAPI sync tab. It renders the review UI for now. */
export function OpenApiSyncTab({ collection }: OpenApiSyncTabProps) {
  return <SpecSyncTab collection={collection} />;
}
