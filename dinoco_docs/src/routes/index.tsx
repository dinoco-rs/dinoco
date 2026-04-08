import React from 'react';

import RouteRedirect from '../components/RouteRedirect';
import { getDefaultVersionName } from '../jsons/versions';

const Home: React.FC = () => {
	return <RouteRedirect to={`/${getDefaultVersionName()}`} />;
};

export default Home;
