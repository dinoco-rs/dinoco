import React, { useEffect } from 'react';
import { useRouter } from 'tuono-router';

type RouteRedirectProps = {
	to: string;
};

const RouteRedirect: React.FC<RouteRedirectProps> = ({ to }) => {
	const router = useRouter();

	useEffect(() => {
		router.replace(to);
	}, [router, to]);

	return null;
};

export default RouteRedirect;
