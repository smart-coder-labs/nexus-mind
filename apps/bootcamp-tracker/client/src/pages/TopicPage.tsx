import React from 'react';
import { useParams, Navigate } from 'react-router-dom';
import { TopicView } from '../components/topic/TopicView';

export function TopicPage() {
  const { id } = useParams<{ id: string }>();
  if (!id || isNaN(parseInt(id))) return <Navigate to="/" />;
  return <TopicView topicId={parseInt(id)} />;
}
