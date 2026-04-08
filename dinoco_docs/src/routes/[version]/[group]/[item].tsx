import React, { useEffect } from 'react';

import DocsPage from '../../../components/DocsPage';
import { getSystemTheme, useDocs } from '../../../hooks/useDocs';

const DocsRoute: React.FC = () => {
	const setTheme = useDocs(state => state.setTheme);

	useEffect(() => {
		console.log(getSystemTheme());

		setTheme(getSystemTheme());
	}, []);

	return <DocsPage />;
};

export default DocsRoute;
